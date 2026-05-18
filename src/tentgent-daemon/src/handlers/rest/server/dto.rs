use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::server::domain::{
    ServerInspection, ServerProcessMetadata, ServerRemoveOutcome, ServerSpec, ServerStopOutcome,
    ServerSummary,
};

#[derive(Debug, Serialize)]
pub struct ServersResponse {
    pub servers: Vec<ServerSummaryItem>,
}

#[derive(Debug, Serialize)]
pub struct ServerResponse {
    pub server: ServerInspectionItem,
}

#[derive(Debug, Serialize)]
pub struct ServerCreateResponse {
    pub server: ServerInspectionItem,
    pub created: bool,
}

#[derive(Debug, Serialize)]
pub struct ServerStartResponse {
    pub server: ServerInspectionItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ServerReadinessItem>,
}

#[derive(Debug, Serialize)]
pub struct ServerStopResponse {
    pub server: ServerInspectionItem,
    pub stopped_pid: u32,
}

#[derive(Debug, Serialize)]
pub struct ServerRemoveResponse {
    pub removed: ServerRemovedItem,
    pub server: ServerInspectionItem,
}

#[derive(Debug, Serialize)]
pub struct ServerRemovedItem {
    pub kind: &'static str,
    pub server_ref: String,
    pub short_ref: String,
    pub server_dir: String,
}

#[derive(Debug, Serialize)]
pub struct ServerSummaryItem {
    pub server_ref: String,
    pub short_ref: String,
    pub runtime_kind: String,
    pub model_ref: Option<String>,
    pub provider: Option<String>,
    pub provider_model: Option<String>,
    pub host: String,
    pub port: u16,
    pub lazy_load: bool,
    pub idle_seconds: Option<u64>,
    pub created_at: String,
    pub running: bool,
    pub process: Option<ServerProcessItem>,
}

#[derive(Debug, Serialize)]
pub struct ServerInspectionItem {
    pub server_ref: String,
    pub short_ref: String,
    pub runtime_kind: String,
    pub model_ref: Option<String>,
    pub provider: Option<String>,
    pub provider_model: Option<String>,
    pub host: String,
    pub port: u16,
    pub lazy_load: bool,
    pub idle_seconds: Option<u64>,
    pub created_at: String,
    pub running: bool,
    pub process: Option<ServerProcessItem>,
    pub home_dir: String,
    pub server_dir: String,
    pub spec_path: String,
    pub process_path: String,
    pub stdout_log: String,
    pub stderr_log: String,
}

#[derive(Debug, Serialize)]
pub struct ServerProcessItem {
    pub pid: u32,
    pub launch_mode: String,
    pub started_at: String,
}

#[derive(Debug, Serialize)]
pub struct ServerHealthResponse {
    pub server: ServerHealthServerItem,
    pub running: bool,
    pub reachable: bool,
    pub target_url: String,
    pub target_status: Option<u16>,
    pub target_health: Option<serde_json::Value>,
    pub checked_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerReadinessItem {
    pub ready: bool,
    pub reachable: bool,
    pub target_status: Option<u16>,
    pub target_health: Option<serde_json::Value>,
    pub checked_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerHealthServerItem {
    pub server_ref: String,
    pub short_ref: String,
    pub running: bool,
}

#[derive(Debug, Serialize)]
pub struct ServerLogsResponse {
    pub logs: ServerLogsItem,
}

#[derive(Debug, Serialize)]
pub struct ServerLogsItem {
    pub stdout: ServerLogMetadataItem,
    pub stderr: ServerLogMetadataItem,
}

#[derive(Debug, Serialize)]
pub struct ServerLogResponse {
    pub log: ServerLogContentItem,
}

#[derive(Debug, Serialize, Clone)]
pub struct ServerLogMetadataItem {
    pub kind: &'static str,
    pub path: String,
    pub exists: bool,
    pub total_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServerLogContentItem {
    pub owner: &'static str,
    pub server_ref: String,
    pub short_ref: String,
    pub kind: &'static str,
    pub path: String,
    pub exists: bool,
    pub total_bytes: u64,
    pub modified_at: Option<String>,
    pub tail_bytes: u64,
    pub truncated: bool,
    pub encoding: &'static str,
    pub content: String,
}

pub fn server_summary_item(summary: ServerSummary) -> ServerSummaryItem {
    let spec = summary.spec;
    let fields = server_fields(spec);
    ServerSummaryItem {
        server_ref: fields.server_ref,
        short_ref: fields.short_ref,
        runtime_kind: fields.runtime_kind,
        model_ref: fields.model_ref,
        provider: fields.provider,
        provider_model: fields.provider_model,
        host: fields.host,
        port: fields.port,
        lazy_load: fields.lazy_load,
        idle_seconds: fields.idle_seconds,
        created_at: fields.created_at,
        running: summary.running,
        process: summary.process.map(server_process_item),
    }
}

pub fn server_inspection_item(inspection: ServerInspection) -> ServerInspectionItem {
    let fields = server_fields(inspection.spec);
    ServerInspectionItem {
        server_ref: fields.server_ref,
        short_ref: fields.short_ref,
        runtime_kind: fields.runtime_kind,
        model_ref: fields.model_ref,
        provider: fields.provider,
        provider_model: fields.provider_model,
        host: fields.host,
        port: fields.port,
        lazy_load: fields.lazy_load,
        idle_seconds: fields.idle_seconds,
        created_at: fields.created_at,
        running: inspection.running,
        process: inspection.process.map(server_process_item),
        home_dir: path_string(&inspection.home_dir),
        server_dir: path_string(&inspection.server_dir),
        spec_path: path_string(&inspection.spec_path),
        process_path: path_string(&inspection.process_path),
        stdout_log: path_string(&inspection.stdout_log_path),
        stderr_log: path_string(&inspection.stderr_log_path),
    }
}

pub fn server_stop_response(outcome: ServerStopOutcome) -> ServerStopResponse {
    ServerStopResponse {
        stopped_pid: outcome.stopped_pid,
        server: server_inspection_item(outcome.inspection),
    }
}

pub fn server_remove_response(outcome: ServerRemoveOutcome) -> ServerRemoveResponse {
    let removed = ServerRemovedItem {
        kind: "server",
        server_ref: outcome.inspection.spec.server_ref.to_string(),
        short_ref: outcome.inspection.spec.short_ref.clone(),
        server_dir: path_string(&outcome.inspection.server_dir),
    };
    ServerRemoveResponse {
        server: server_inspection_item(outcome.inspection),
        removed,
    }
}

pub fn server_health_server_item(inspection: &ServerInspection) -> ServerHealthServerItem {
    ServerHealthServerItem {
        server_ref: inspection.spec.server_ref.to_string(),
        short_ref: inspection.spec.short_ref.clone(),
        running: inspection.running,
    }
}

struct ServerFields {
    server_ref: String,
    short_ref: String,
    runtime_kind: String,
    model_ref: Option<String>,
    provider: Option<String>,
    provider_model: Option<String>,
    host: String,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
    created_at: String,
}

fn server_fields(spec: ServerSpec) -> ServerFields {
    ServerFields {
        server_ref: spec.server_ref.into_string(),
        short_ref: spec.short_ref,
        runtime_kind: spec.runtime_kind.to_string(),
        model_ref: spec.model_ref.map(|model_ref| model_ref.into_string()),
        provider: spec.provider.map(|provider| provider.to_string()),
        provider_model: spec.provider_model,
        host: spec.host,
        port: spec.port,
        lazy_load: spec.lazy_load,
        idle_seconds: spec.idle_seconds,
        created_at: spec.created_at,
    }
}

fn server_process_item(process: ServerProcessMetadata) -> ServerProcessItem {
    ServerProcessItem {
        pid: process.pid,
        launch_mode: process.launch_mode.to_string(),
        started_at: process.started_at,
    }
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
