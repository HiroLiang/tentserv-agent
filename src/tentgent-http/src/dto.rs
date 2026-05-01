use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse<'a> {
    pub(crate) status: &'a str,
    pub(crate) service: &'a str,
    pub(crate) version: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct StatusResponse {
    pub(crate) service: &'static str,
    pub(crate) version: &'static str,
    pub(crate) status: &'static str,
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) pid: Option<u32>,
    pub(crate) started_at: Option<String>,
    pub(crate) runtime_home: String,
    pub(crate) runtime_dir: String,
    pub(crate) log_dir: String,
    pub(crate) process_path: String,
    pub(crate) pid_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModelsResponse {
    pub(crate) models: Vec<ModelItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModelItem {
    pub(crate) model_ref: String,
    pub(crate) short_ref: String,
    pub(crate) store_path: String,
    pub(crate) file_count: usize,
    pub(crate) total_bytes: u64,
    pub(crate) imported_at: String,
    pub(crate) format: String,
    pub(crate) detected_formats: Vec<String>,
    pub(crate) source_kind: String,
    pub(crate) source_repo: Option<String>,
    pub(crate) source_revision: Option<String>,
    pub(crate) source_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AdaptersResponse {
    pub(crate) adapters: Vec<AdapterItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AdapterItem {
    pub(crate) adapter_ref: String,
    pub(crate) short_ref: String,
    pub(crate) store_path: String,
    pub(crate) file_count: usize,
    pub(crate) total_bytes: u64,
    pub(crate) imported_at: String,
    pub(crate) format: String,
    #[serde(rename = "type")]
    pub(crate) adapter_type: String,
    pub(crate) base_model_ref: Option<String>,
    pub(crate) base_model_source_repo: Option<String>,
    pub(crate) base_model_source_revision: Option<String>,
    pub(crate) model_family: Option<String>,
    pub(crate) backend_support: Vec<String>,
    pub(crate) source_kind: String,
    pub(crate) source_repo: Option<String>,
    pub(crate) source_revision: Option<String>,
    pub(crate) source_path: Option<String>,
    pub(crate) training_dataset_ref: Option<String>,
    pub(crate) training_run_ref: Option<String>,
    pub(crate) training_config_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetsResponse {
    pub(crate) datasets: Vec<DatasetItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetItem {
    pub(crate) dataset_ref: String,
    pub(crate) short_ref: String,
    pub(crate) store_path: String,
    pub(crate) file_count: usize,
    pub(crate) total_bytes: u64,
    pub(crate) imported_at: String,
    pub(crate) format: String,
    pub(crate) source_kind: String,
    pub(crate) source_path: Option<String>,
    pub(crate) source_repo: Option<String>,
    pub(crate) source_revision: Option<String>,
    pub(crate) tuning_ready: bool,
    pub(crate) splits: DatasetSplitsItem,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetSplitsItem {
    pub(crate) train: Option<String>,
    pub(crate) validation: Option<String>,
    pub(crate) test: Option<String>,
    pub(crate) eval_cases: Option<String>,
    pub(crate) source_manifest: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServersResponse {
    pub(crate) servers: Vec<ServerSummaryItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerResponse {
    pub(crate) server: ServerInspectionItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerStartResponse {
    pub(crate) server: ServerInspectionItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) readiness: Option<ServerReadinessItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateServerResponse {
    pub(crate) server: ServerInspectionItem,
    pub(crate) created: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct StopServerResponse {
    pub(crate) server: ServerInspectionItem,
    pub(crate) stopped_pid: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerHealthResponse {
    pub(crate) server: ServerInspectionItem,
    pub(crate) running: bool,
    pub(crate) reachable: bool,
    pub(crate) target_url: String,
    pub(crate) target_status: Option<u16>,
    pub(crate) target_health: Option<Value>,
    pub(crate) checked_at: String,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerReadinessItem {
    pub(crate) ready: bool,
    pub(crate) reachable: bool,
    pub(crate) target_status: Option<u16>,
    pub(crate) target_health: Option<Value>,
    pub(crate) checked_at: String,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerSummaryItem {
    pub(crate) server_ref: String,
    pub(crate) short_ref: String,
    pub(crate) runtime_kind: String,
    pub(crate) model_ref: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) provider_model: Option<String>,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) lazy_load: bool,
    pub(crate) idle_seconds: Option<u64>,
    pub(crate) created_at: String,
    pub(crate) running: bool,
    pub(crate) process: Option<ServerProcessItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerInspectionItem {
    pub(crate) server_ref: String,
    pub(crate) short_ref: String,
    pub(crate) runtime_kind: String,
    pub(crate) model_ref: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) provider_model: Option<String>,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) lazy_load: bool,
    pub(crate) idle_seconds: Option<u64>,
    pub(crate) created_at: String,
    pub(crate) running: bool,
    pub(crate) process: Option<ServerProcessItem>,
    pub(crate) home_dir: String,
    pub(crate) server_dir: String,
    pub(crate) spec_path: String,
    pub(crate) process_path: String,
    pub(crate) stdout_log: String,
    pub(crate) stderr_log: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerProcessItem {
    pub(crate) pid: u32,
    pub(crate) launch_mode: String,
    pub(crate) started_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ErrorResponse<'a> {
    pub(crate) error: &'a str,
    pub(crate) message: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StartServerRequest {
    pub(crate) wait_ready: Option<bool>,
    pub(crate) timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StartServerOptions {
    pub(crate) wait_ready: bool,
    pub(crate) timeout: Duration,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateServerRequest {
    pub(crate) runtime_ref: String,
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
    #[serde(default)]
    pub(crate) lazy_load: bool,
    pub(crate) idle_seconds: Option<u64>,
}
