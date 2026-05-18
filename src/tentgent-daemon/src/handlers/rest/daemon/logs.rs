use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use axum::{
    extract::{RawQuery, State},
    Json,
};
use tentgent_kernel::{
    features::daemon::{
        domain::DaemonInspection,
        usecases::{DaemonInspectionMode, DaemonStatusRequest, DaemonStatusUseCase},
    },
    foundation::layout::LayoutResolveMode,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::transport::rest::{error::RestError, state::RestState};

use super::dto;

const DEFAULT_LOG_TAIL_BYTES: u64 = 65_536;
const MAX_LOG_TAIL_BYTES: u64 = 262_144;

pub async fn logs(
    State(state): State<RestState>,
) -> Result<Json<dto::DaemonLogsResponse>, RestError> {
    let inspection = inspect_daemon(&state)?;
    Ok(Json(daemon_logs_response_body(
        daemon_log_metadata("stdout", &inspection.stdout_log_path)?,
        daemon_log_metadata("stderr", &inspection.stderr_log_path)?,
    )))
}

pub async fn stdout_log(
    State(state): State<RestState>,
    RawQuery(query): RawQuery,
) -> Result<Json<dto::DaemonLogResponse>, RestError> {
    let tail_bytes = log_tail_bytes(query.as_deref())?;
    let inspection = inspect_daemon(&state)?;
    Ok(Json(daemon_log_response_body(daemon_log_tail(
        "stdout",
        &inspection.stdout_log_path,
        tail_bytes,
    )?)))
}

pub async fn stderr_log(
    State(state): State<RestState>,
    RawQuery(query): RawQuery,
) -> Result<Json<dto::DaemonLogResponse>, RestError> {
    let tail_bytes = log_tail_bytes(query.as_deref())?;
    let inspection = inspect_daemon(&state)?;
    Ok(Json(daemon_log_response_body(daemon_log_tail(
        "stderr",
        &inspection.stderr_log_path,
        tail_bytes,
    )?)))
}

struct DaemonLogTail {
    metadata: dto::DaemonLogMetadataItem,
    tail_bytes: u64,
    truncated: bool,
    content: String,
}

fn inspect_daemon(state: &RestState) -> Result<DaemonInspection, RestError> {
    let status = state
        .app()
        .services()
        .daemon()
        .usecase()
        .daemon_status(DaemonStatusRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            mode: DaemonInspectionMode::Observational,
        })
        .map_err(|err| RestError::kernel("daemon_status_failed", err))?;
    Ok(status.inspection)
}

fn daemon_logs_response_body(
    stdout: dto::DaemonLogMetadataItem,
    stderr: dto::DaemonLogMetadataItem,
) -> dto::DaemonLogsResponse {
    dto::DaemonLogsResponse {
        logs: dto::DaemonLogsItem { stdout, stderr },
    }
}

fn daemon_log_response_body(log: DaemonLogTail) -> dto::DaemonLogResponse {
    dto::DaemonLogResponse {
        log: dto::DaemonLogContentItem {
            owner: "daemon",
            server_ref: None,
            short_ref: None,
            kind: log.metadata.kind,
            path: log.metadata.path,
            exists: log.metadata.exists,
            total_bytes: log.metadata.total_bytes,
            modified_at: log.metadata.modified_at,
            tail_bytes: log.tail_bytes,
            truncated: log.truncated,
            encoding: "utf-8-lossy",
            content: log.content,
        },
    }
}

fn daemon_log_tail(
    kind: &'static str,
    path: &Path,
    tail_bytes: u64,
) -> Result<DaemonLogTail, RestError> {
    let metadata = daemon_log_metadata(kind, path)?;
    if !metadata.exists {
        return Ok(DaemonLogTail {
            metadata,
            tail_bytes,
            truncated: false,
            content: String::new(),
        });
    }

    let read_from = metadata.total_bytes.saturating_sub(tail_bytes);
    let mut file = File::open(path).map_err(|err| log_io_error("open daemon log", path, err))?;
    file.seek(SeekFrom::Start(read_from))
        .map_err(|err| log_io_error("seek daemon log", path, err))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|err| log_io_error("read daemon log", path, err))?;

    Ok(DaemonLogTail {
        truncated: metadata.total_bytes > tail_bytes,
        metadata,
        tail_bytes,
        content: String::from_utf8_lossy(&bytes).to_string(),
    })
}

fn daemon_log_metadata(
    kind: &'static str,
    path: &Path,
) -> Result<dto::DaemonLogMetadataItem, RestError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(dto::DaemonLogMetadataItem {
            kind,
            path: path.display().to_string(),
            exists: true,
            total_bytes: metadata.len(),
            modified_at: metadata.modified().ok().and_then(system_time_rfc3339),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(dto::DaemonLogMetadataItem {
            kind,
            path: path.display().to_string(),
            exists: false,
            total_bytes: 0,
            modified_at: None,
        }),
        Err(err) => Err(log_io_error("read daemon log metadata", path, err)),
    }
}

fn log_tail_bytes(query: Option<&str>) -> Result<u64, RestError> {
    let values = query_values(query, "tail_bytes");
    match values.as_slice() {
        [] => Ok(DEFAULT_LOG_TAIL_BYTES),
        [value] => parse_log_tail_bytes(value),
        _ => Err(RestError::bad_request(
            "bad_request",
            "`tail_bytes` must be provided at most once",
        )),
    }
}

fn parse_log_tail_bytes(value: &str) -> Result<u64, RestError> {
    let parsed = value.parse::<u64>().map_err(|_| {
        RestError::bad_request(
            "bad_request",
            format!("`tail_bytes` must be an integer between 1 and {MAX_LOG_TAIL_BYTES}"),
        )
    })?;
    if parsed == 0 || parsed > MAX_LOG_TAIL_BYTES {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`tail_bytes` must be between 1 and {MAX_LOG_TAIL_BYTES}"),
        ));
    }
    Ok(parsed)
}

fn query_values<'a>(query: Option<&'a str>, key: &'static str) -> Vec<&'a str> {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter_map(|part| {
            let (name, value) = part.split_once('=')?;
            (name == key).then_some(value)
        })
        .collect()
}

fn system_time_rfc3339(value: std::time::SystemTime) -> Option<String> {
    OffsetDateTime::from(value).format(&Rfc3339).ok()
}

fn log_io_error(action: &str, path: &Path, err: std::io::Error) -> RestError {
    RestError::internal(
        "log_read_failed",
        format!("failed to {action} `{}`: {err}", path.display()),
    )
}
