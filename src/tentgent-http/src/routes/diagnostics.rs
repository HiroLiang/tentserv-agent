use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::SystemTime,
};

use tentgent_core::server::{ServerInspection, ServerManager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    app::DaemonHttpState,
    dto::{ErrorResponse, LogContentItem, LogMetadataItem, LogPairItem, LogResponse, LogsResponse},
    http::{HttpRequest, HttpResponse},
    response::{bad_request_response, json_response, not_found_response, server_error_response},
    routes::store::path_string,
};

const DEFAULT_TAIL_BYTES: u64 = 65_536;
const MAX_TAIL_BYTES: u64 = 262_144;
const LOG_ENCODING: &str = "utf-8-lossy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogKind {
    Stdout,
    Stderr,
}

impl LogKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "stdout" => Some(Self::Stdout),
            "stderr" => Some(Self::Stderr),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerLogsPath<'a> {
    Metadata(&'a str),
    Content { reference: &'a str, kind: LogKind },
    Invalid,
}

pub(crate) fn daemon_logs_metadata_response(state: &DaemonHttpState) -> HttpResponse {
    logs_metadata_response(
        &state.inspection().stdout_log_path,
        &state.inspection().stderr_log_path,
    )
}

pub(crate) fn daemon_log_content_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    kind: &str,
) -> HttpResponse {
    let Some(kind) = LogKind::parse(kind) else {
        return not_found_response(&request.path);
    };
    let tail_bytes = match tail_bytes(request) {
        Ok(tail_bytes) => tail_bytes,
        Err(response) => return response,
    };
    let path = daemon_log_path(state, kind);

    log_content_response(
        LogOwner {
            owner: "daemon".to_string(),
            server_ref: None,
            short_ref: None,
        },
        kind,
        path,
        tail_bytes,
    )
}

pub(crate) fn is_server_logs_path(path: &str) -> bool {
    parse_server_logs_path(path).is_some()
}

pub(crate) fn server_logs_response(state: &DaemonHttpState, request: &HttpRequest) -> HttpResponse {
    match parse_server_logs_path(&request.path) {
        Some(ServerLogsPath::Metadata(reference)) => {
            let inspection = match server_inspection(state, reference) {
                Ok(inspection) => inspection,
                Err(response) => return response,
            };
            logs_metadata_response(&inspection.stdout_log_path, &inspection.stderr_log_path)
        }
        Some(ServerLogsPath::Content { reference, kind }) => {
            let tail_bytes = match tail_bytes(request) {
                Ok(tail_bytes) => tail_bytes,
                Err(response) => return response,
            };
            let inspection = match server_inspection(state, reference) {
                Ok(inspection) => inspection,
                Err(response) => return response,
            };
            let path = server_log_path(&inspection, kind).to_path_buf();
            let server_ref = inspection.spec.server_ref;
            let short_ref = inspection.spec.short_ref;
            log_content_response(
                LogOwner {
                    owner: "server".to_string(),
                    server_ref: Some(server_ref),
                    short_ref: Some(short_ref),
                },
                kind,
                &path,
                tail_bytes,
            )
        }
        Some(ServerLogsPath::Invalid) => not_found_response(&request.path),
        None => not_found_response(&request.path),
    }
}

fn logs_metadata_response(stdout_path: &Path, stderr_path: &Path) -> HttpResponse {
    let stdout = match log_metadata(LogKind::Stdout, stdout_path) {
        Ok(metadata) => metadata,
        Err(response) => return response,
    };
    let stderr = match log_metadata(LogKind::Stderr, stderr_path) {
        Ok(metadata) => metadata,
        Err(response) => return response,
    };

    json_response(
        200,
        LogsResponse {
            logs: LogPairItem { stdout, stderr },
        },
    )
}

fn log_content_response(
    owner: LogOwner,
    kind: LogKind,
    path: &Path,
    tail_bytes: u64,
) -> HttpResponse {
    let metadata = match log_metadata(kind, path) {
        Ok(metadata) => metadata,
        Err(response) => return response,
    };
    if !metadata.exists {
        return json_response(
            200,
            LogResponse {
                log: LogContentItem {
                    owner: owner.owner,
                    server_ref: owner.server_ref,
                    short_ref: owner.short_ref,
                    kind: metadata.kind,
                    path: metadata.path,
                    exists: false,
                    total_bytes: 0,
                    modified_at: None,
                    tail_bytes,
                    truncated: false,
                    encoding: LOG_ENCODING.to_string(),
                    content: String::new(),
                },
            },
        );
    }

    let content = match read_tail(path, metadata.total_bytes, tail_bytes) {
        Ok(content) => content,
        Err(response) => return response,
    };
    let truncated = metadata.total_bytes > tail_bytes;

    json_response(
        200,
        LogResponse {
            log: LogContentItem {
                owner: owner.owner,
                server_ref: owner.server_ref,
                short_ref: owner.short_ref,
                kind: metadata.kind,
                path: metadata.path,
                exists: metadata.exists,
                total_bytes: metadata.total_bytes,
                modified_at: metadata.modified_at,
                tail_bytes,
                truncated,
                encoding: LOG_ENCODING.to_string(),
                content,
            },
        },
    )
}

fn log_metadata(kind: LogKind, path: &Path) -> Result<LogMetadataItem, HttpResponse> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LogMetadataItem {
                kind: kind.as_str().to_string(),
                path: path_string(path),
                exists: false,
                total_bytes: 0,
                modified_at: None,
            })
        }
        Err(error) => {
            return Err(log_read_failed(format!(
                "failed to read log metadata `{}`: {error}",
                path_string(path)
            )))
        }
    };

    let modified_at = match metadata.modified() {
        Ok(modified) => Some(format_system_time(modified)?),
        Err(error) => {
            return Err(log_read_failed(format!(
                "failed to read log modified time `{}`: {error}",
                path_string(path)
            )))
        }
    };

    Ok(LogMetadataItem {
        kind: kind.as_str().to_string(),
        path: path_string(path),
        exists: true,
        total_bytes: metadata.len(),
        modified_at,
    })
}

fn read_tail(path: &Path, total_bytes: u64, tail_bytes: u64) -> Result<String, HttpResponse> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(String::new());
        }
        Err(error) => {
            return Err(log_read_failed(format!(
                "failed to open log `{}`: {error}",
                path_string(path)
            )))
        }
    };

    let read_from = total_bytes.saturating_sub(tail_bytes);
    file.seek(SeekFrom::Start(read_from)).map_err(|error| {
        log_read_failed(format!(
            "failed to seek log `{}`: {error}",
            path_string(path)
        ))
    })?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|error| {
        log_read_failed(format!(
            "failed to read log `{}`: {error}",
            path_string(path)
        ))
    })?;

    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn server_inspection(
    state: &DaemonHttpState,
    reference: &str,
) -> Result<ServerInspection, HttpResponse> {
    let manager =
        ServerManager::open_readonly(Some(state.home_dir())).map_err(server_error_response)?;
    manager.inspect(reference).map_err(server_error_response)
}

fn daemon_log_path(state: &DaemonHttpState, kind: LogKind) -> &Path {
    match kind {
        LogKind::Stdout => &state.inspection().stdout_log_path,
        LogKind::Stderr => &state.inspection().stderr_log_path,
    }
}

fn server_log_path(inspection: &ServerInspection, kind: LogKind) -> &Path {
    match kind {
        LogKind::Stdout => &inspection.stdout_log_path,
        LogKind::Stderr => &inspection.stderr_log_path,
    }
}

fn tail_bytes(request: &HttpRequest) -> Result<u64, HttpResponse> {
    let values = request.query_values("tail_bytes").collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(DEFAULT_TAIL_BYTES),
        [value] => parse_tail_bytes(value),
        _ => Err(bad_request_response(
            "`tail_bytes` must be provided at most once",
        )),
    }
}

fn parse_tail_bytes(value: &str) -> Result<u64, HttpResponse> {
    let parsed = value.parse::<u64>().map_err(|_| {
        bad_request_response(format!(
            "`tail_bytes` must be an integer between 1 and {MAX_TAIL_BYTES}"
        ))
    })?;
    if parsed == 0 || parsed > MAX_TAIL_BYTES {
        return Err(bad_request_response(format!(
            "`tail_bytes` must be between 1 and {MAX_TAIL_BYTES}"
        )));
    }
    Ok(parsed)
}

fn parse_server_logs_path(path: &str) -> Option<ServerLogsPath<'_>> {
    let rest = path.strip_prefix("/v1/servers/")?;
    if let Some(reference) = rest.strip_suffix("/logs") {
        return Some(if valid_server_reference_path(reference) {
            ServerLogsPath::Metadata(reference)
        } else {
            ServerLogsPath::Invalid
        });
    }

    let Some((reference, kind)) = rest.split_once("/logs/") else {
        return None;
    };
    if !valid_server_reference_path(reference) || kind.contains('/') {
        return Some(ServerLogsPath::Invalid);
    }

    Some(match LogKind::parse(kind) {
        Some(kind) => ServerLogsPath::Content { reference, kind },
        None => ServerLogsPath::Invalid,
    })
}

fn valid_server_reference_path(reference: &str) -> bool {
    !reference.is_empty() && !reference.contains('/')
}

fn format_system_time(system_time: SystemTime) -> Result<String, HttpResponse> {
    OffsetDateTime::from(system_time)
        .format(&Rfc3339)
        .map_err(|error| log_read_failed(format!("failed to format log modified time: {error}")))
}

fn log_read_failed(message: impl Into<String>) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "log_read_failed",
            message: message.into(),
        },
    )
}

struct LogOwner {
    owner: String,
    server_ref: Option<String>,
    short_ref: Option<String>,
}
