use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::server::domain::{
    ServerInspection, ServerProcessMetadata, ServerSpec, ServerSummary,
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
