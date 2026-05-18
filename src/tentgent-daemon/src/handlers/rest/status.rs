use axum::{extract::State, Json};
use serde::Serialize;
use tentgent_kernel::{
    features::daemon::{
        domain::{DaemonInspection, DaemonWarning},
        usecases::{DaemonInspectionMode, DaemonStatusRequest, DaemonStatusUseCase},
    },
    foundation::layout::LayoutResolveMode,
};

use crate::transport::rest::{
    error::RestError, response::SERVICE_NAME, security::daemon_token_enabled, state::RestState,
};

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub service: &'static str,
    pub version: &'static str,
    pub status: &'static str,
    pub auth: StatusAuthItem,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub started_at: Option<String>,
    pub runtime_home: String,
    pub runtime_dir: String,
    pub log_dir: String,
    pub process_path: String,
    pub pid_path: String,
    pub warnings: Vec<StatusWarningItem>,
}

#[derive(Debug, Serialize)]
pub struct StatusAuthItem {
    pub token_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct StatusWarningItem {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

pub async fn status(State(state): State<RestState>) -> Result<Json<StatusResponse>, RestError> {
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

    Ok(Json(status_response(status.inspection)))
}

fn status_response(inspection: DaemonInspection) -> StatusResponse {
    let process = inspection.process.as_ref();
    StatusResponse {
        service: SERVICE_NAME,
        version: env!("CARGO_PKG_VERSION"),
        status: if inspection.running {
            "running"
        } else {
            "stopped"
        },
        auth: StatusAuthItem {
            token_enabled: daemon_token_enabled(),
        },
        host: process.map(|process| process.host.clone()),
        port: process.map(|process| process.port),
        pid: process.map(|process| process.pid),
        started_at: process.map(|process| process.started_at.clone()),
        runtime_home: path_string(&inspection.home_dir),
        runtime_dir: path_string(&inspection.runtime_dir),
        log_dir: path_string(&inspection.log_dir),
        process_path: path_string(&inspection.process_path),
        pid_path: path_string(&inspection.pid_path),
        warnings: warning_items(inspection.warnings),
    }
}

fn warning_items(warnings: Vec<DaemonWarning>) -> Vec<StatusWarningItem> {
    warnings
        .into_iter()
        .map(|warning| StatusWarningItem {
            code: warning.code,
            message: warning.message,
            path: warning.path.as_ref().map(path_string),
        })
        .collect()
}

fn path_string(path: impl AsRef<std::path::Path>) -> String {
    path.as_ref().display().to_string()
}
