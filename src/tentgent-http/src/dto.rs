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
    pub(crate) auth: StatusAuthItem,
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
pub(crate) struct StatusAuthItem {
    pub(crate) token_enabled: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthProvidersResponse {
    pub(crate) providers: Vec<AuthProviderItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthProviderResponse {
    pub(crate) provider: AuthProviderItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthProviderItem {
    pub(crate) provider: String,
    pub(crate) display_name: String,
    pub(crate) env_present: bool,
    pub(crate) keychain_present: bool,
    pub(crate) effective_source: Option<String>,
    pub(crate) validation: AuthValidationItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthValidationItem {
    pub(crate) state: String,
    pub(crate) summary: String,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorResponse {
    pub(crate) status: String,
    pub(crate) summary: DoctorSummaryItem,
    pub(crate) checks: Vec<DoctorCheckItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorSummaryItem {
    pub(crate) pass: usize,
    pub(crate) warn: usize,
    pub(crate) fail: usize,
    pub(crate) skipped: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorCheckItem {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DaemonShutdownResponse {
    pub(crate) shutdown: DaemonShutdownItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct DaemonShutdownItem {
    pub(crate) accepted: bool,
    pub(crate) pid: Option<u32>,
    pub(crate) message: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct LogsResponse {
    pub(crate) logs: LogPairItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct LogPairItem {
    pub(crate) stdout: LogMetadataItem,
    pub(crate) stderr: LogMetadataItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct LogMetadataItem {
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) total_bytes: u64,
    pub(crate) modified_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct LogResponse {
    pub(crate) log: LogContentItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct LogContentItem {
    pub(crate) owner: String,
    pub(crate) server_ref: Option<String>,
    pub(crate) short_ref: Option<String>,
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) total_bytes: u64,
    pub(crate) modified_at: Option<String>,
    pub(crate) tail_bytes: u64,
    pub(crate) truncated: bool,
    pub(crate) encoding: String,
    pub(crate) content: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModelsResponse {
    pub(crate) models: Vec<ModelItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModelResponse {
    pub(crate) model: ModelItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct ModelMutationResponse {
    pub(crate) model: ModelItem,
    pub(crate) mutation: StoreMutationItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoveModelResponse {
    pub(crate) removed: RemovedModelItem,
    pub(crate) model: ModelItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemovedModelItem {
    pub(crate) kind: &'static str,
    pub(crate) model_ref: String,
    pub(crate) short_ref: String,
    pub(crate) store_path: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) variant_source_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AdaptersResponse {
    pub(crate) adapters: Vec<AdapterItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AdapterResponse {
    pub(crate) adapter: AdapterItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct AdapterMutationResponse {
    pub(crate) adapter: AdapterItem,
    pub(crate) mutation: StoreMutationItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoveAdapterResponse {
    pub(crate) removed: RemovedAdapterItem,
    pub(crate) adapter: AdapterItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemovedAdapterItem {
    pub(crate) kind: &'static str,
    pub(crate) adapter_ref: String,
    pub(crate) short_ref: String,
    pub(crate) store_path: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) managed_source_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetsResponse {
    pub(crate) datasets: Vec<DatasetItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetResponse {
    pub(crate) dataset: DatasetItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetMutationResponse {
    pub(crate) dataset: DatasetItem,
    pub(crate) mutation: StoreMutationItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetValidationResponse {
    pub(crate) valid: bool,
    pub(crate) source: DatasetToolSourceItem,
    pub(crate) target: String,
    pub(crate) tuning_ready: bool,
    pub(crate) records: usize,
    pub(crate) errors_count: usize,
    pub(crate) splits: Vec<DatasetValidationSplitItem>,
    pub(crate) warnings: Vec<String>,
    pub(crate) errors: Vec<DatasetValidationErrorItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetToolSourceItem {
    pub(crate) kind: &'static str,
    pub(crate) path: String,
    pub(crate) dataset_ref: Option<String>,
    pub(crate) short_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetValidationSplitItem {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) records: usize,
    pub(crate) errors: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetValidationErrorItem {
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetTemplateResponse {
    pub(crate) template_version: &'static str,
    pub(crate) task: String,
    pub(crate) language: String,
    pub(crate) content: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetExportResponse {
    pub(crate) dataset: DatasetItem,
    pub(crate) export: DatasetExportItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetExportItem {
    pub(crate) output_path: String,
    pub(crate) managed_source_path: String,
    pub(crate) file_count: usize,
    pub(crate) total_bytes: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetDiffResponse {
    pub(crate) left: DatasetDiffSideItem,
    pub(crate) right: DatasetDiffSideItem,
    pub(crate) diff: DatasetDiffItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetDiffSideItem {
    pub(crate) label: String,
    pub(crate) short_ref: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) tuning_ready: bool,
    pub(crate) splits: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetDiffItem {
    pub(crate) summary: DatasetDiffSummaryItem,
    pub(crate) files: Vec<DatasetDiffFileItem>,
    pub(crate) file_limit: usize,
    pub(crate) truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetDiffSummaryItem {
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) modified: usize,
    pub(crate) unchanged: usize,
    pub(crate) left_total_bytes: u64,
    pub(crate) right_total_bytes: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetDiffFileItem {
    pub(crate) status: String,
    pub(crate) relative_path: String,
    pub(crate) left_size_bytes: Option<u64>,
    pub(crate) right_size_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetSynthPromptResponse {
    pub(crate) prompt: DatasetSynthPromptItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetSynthPromptItem {
    pub(crate) content: String,
    pub(crate) split: String,
    pub(crate) source_kind: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetSynthResponse {
    pub(crate) synth: Value,
    pub(crate) progress_events: Vec<Value>,
    pub(crate) progress_truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetEvalResponse {
    #[serde(rename = "eval")]
    pub(crate) evaluation: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetRuntimeDebugItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) debug_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prompt_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider_output_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoveDatasetResponse {
    pub(crate) removed: RemovedDatasetItem,
    pub(crate) dataset: DatasetItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemovedDatasetItem {
    pub(crate) kind: &'static str,
    pub(crate) dataset_ref: String,
    pub(crate) short_ref: String,
    pub(crate) store_path: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) managed_source_path: Option<String>,
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
pub(crate) struct StoreMutationItem {
    pub(crate) kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deduplicated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) store_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source_index_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_index_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) base_model_ref: Option<String>,
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
pub(crate) struct RemoveServerResponse {
    pub(crate) removed: RemovedServerItem,
    pub(crate) server: ServerInspectionItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemovedServerItem {
    pub(crate) kind: &'static str,
    pub(crate) server_ref: String,
    pub(crate) short_ref: String,
    pub(crate) server_dir: String,
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
pub(crate) struct SessionsResponse {
    pub(crate) sessions: Vec<SessionSummaryItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionResponse {
    pub(crate) session: SessionInspectionItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionMessagesResponse {
    pub(crate) session: SessionRefItem,
    pub(crate) messages: Vec<SessionMessageItem>,
    pub(crate) tail: usize,
    pub(crate) total_messages: usize,
    pub(crate) truncated: bool,
    pub(crate) warnings: Vec<SessionWarningItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionSummaryItem {
    pub(crate) session_ref: String,
    pub(crate) short_ref: String,
    pub(crate) title: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) message_count: usize,
    pub(crate) default_server_ref: Option<String>,
    pub(crate) adapter_ref: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) store_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionInspectionItem {
    pub(crate) session_ref: String,
    pub(crate) short_ref: String,
    pub(crate) title: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) message_count: usize,
    pub(crate) default_server_ref: Option<String>,
    pub(crate) adapter_ref: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) store_path: String,
    pub(crate) messages_path: String,
    pub(crate) warnings: Vec<SessionWarningItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionRefItem {
    pub(crate) session_ref: String,
    pub(crate) short_ref: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionMessageItem {
    pub(crate) index: usize,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) created_at: String,
    pub(crate) server_ref: Option<String>,
    pub(crate) adapter_ref: Option<String>,
    pub(crate) metadata: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionWarningItem {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainPlansResponse {
    pub(crate) plans: Vec<TrainPlanSummaryItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainPlanPreviewResponse {
    pub(crate) plan: TrainPlanItem,
    pub(crate) preview: TrainPlanPreviewItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainPlanCreateResponse {
    pub(crate) plan: TrainPlanItem,
    pub(crate) created: bool,
    pub(crate) deduplicated: bool,
    pub(crate) run_count: usize,
    pub(crate) plan_dir: String,
    pub(crate) plan_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainPlanResponse {
    pub(crate) plan: TrainPlanItem,
    pub(crate) run_count: usize,
    pub(crate) plan_dir: String,
    pub(crate) plan_path: String,
    pub(crate) runs_dir: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoveTrainPlanResponse {
    pub(crate) removed: RemovedTrainPlanItem,
    pub(crate) plan: TrainPlanItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemovedTrainPlanItem {
    pub(crate) kind: &'static str,
    pub(crate) plan_ref: String,
    pub(crate) short_ref: String,
    pub(crate) plan_dir: String,
    pub(crate) run_count: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainPlanPreviewItem {
    pub(crate) would_reuse: bool,
    pub(crate) persisted: bool,
    pub(crate) run_count: usize,
    pub(crate) would_plan_dir: String,
    pub(crate) would_plan_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainPlanSummaryItem {
    pub(crate) plan_ref: String,
    pub(crate) short_ref: String,
    pub(crate) name: Option<String>,
    pub(crate) status: String,
    pub(crate) requested_backend: String,
    pub(crate) backend: Option<String>,
    pub(crate) model_ref: String,
    pub(crate) dataset_ref: String,
    pub(crate) created_at: String,
    pub(crate) run_count: usize,
    pub(crate) plan_dir: String,
    pub(crate) plan_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainPlanItem {
    pub(crate) schema_version: u32,
    pub(crate) plan_ref: String,
    pub(crate) short_ref: String,
    pub(crate) name: Option<String>,
    pub(crate) status: String,
    pub(crate) created_at: String,
    pub(crate) model_ref: String,
    pub(crate) model_short_ref: String,
    pub(crate) dataset_ref: String,
    pub(crate) dataset_short_ref: String,
    pub(crate) requested_backend: String,
    pub(crate) backend: Option<String>,
    pub(crate) profile: String,
    pub(crate) selection_reason: String,
    pub(crate) blockers: Vec<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) model: Value,
    pub(crate) dataset: Value,
    pub(crate) lora: Value,
    pub(crate) optimization: Value,
    pub(crate) checkpoint: Value,
    pub(crate) output: Value,
    pub(crate) backend_config: Value,
    pub(crate) command_hint: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunStartResponse {
    pub(crate) run: TrainRunItem,
    pub(crate) plan: TrainPlanItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunsResponse {
    pub(crate) runs: Vec<TrainRunSummaryItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunResponse {
    pub(crate) run: TrainRunItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunMetricsResponse {
    pub(crate) run: TrainRunRefItem,
    pub(crate) metrics_path: String,
    pub(crate) tail: usize,
    pub(crate) total_events: usize,
    pub(crate) truncated: bool,
    pub(crate) events: Vec<TrainRunMetricItem>,
    pub(crate) warnings: Vec<TrainRunWarningItem>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunLogsResponse {
    pub(crate) logs: TrainRunLogsItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunLogsItem {
    pub(crate) raw: TrainRunLogMetadataItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunLogResponse {
    pub(crate) log: TrainRunLogContentItem,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunLogMetadataItem {
    pub(crate) kind: &'static str,
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) total_bytes: u64,
    pub(crate) modified_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunLogContentItem {
    pub(crate) owner: &'static str,
    pub(crate) run_ref: String,
    pub(crate) short_ref: String,
    pub(crate) kind: &'static str,
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) total_bytes: u64,
    pub(crate) modified_at: Option<String>,
    pub(crate) tail_bytes: u64,
    pub(crate) truncated: bool,
    pub(crate) encoding: &'static str,
    pub(crate) content: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunRefItem {
    pub(crate) run_ref: String,
    pub(crate) short_ref: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunMetricItem {
    pub(crate) index: usize,
    pub(crate) event: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunWarningItem {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) line: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunSummaryItem {
    pub(crate) run_ref: String,
    pub(crate) short_ref: String,
    pub(crate) status: String,
    pub(crate) process_running: bool,
    pub(crate) stale: bool,
    pub(crate) phase: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) plan_ref: String,
    pub(crate) model_ref: String,
    pub(crate) dataset_ref: String,
    pub(crate) backend: Option<String>,
    pub(crate) pid: Option<u32>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) adapter_ref: Option<String>,
    pub(crate) created_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) ended_at: Option<String>,
    pub(crate) run_dir: String,
    pub(crate) run_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrainRunItem {
    pub(crate) run_ref: String,
    pub(crate) short_ref: String,
    pub(crate) status: String,
    pub(crate) process_running: bool,
    pub(crate) stale: bool,
    pub(crate) phase: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) plan_ref: String,
    pub(crate) plan_short_ref: String,
    pub(crate) model_ref: String,
    pub(crate) dataset_ref: String,
    pub(crate) backend: Option<String>,
    pub(crate) recipe_hash: String,
    pub(crate) pid: Option<u32>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) exit_signal: Option<String>,
    pub(crate) adapter_ref: Option<String>,
    pub(crate) adapter_path: Option<String>,
    pub(crate) adapter_output_path: Option<String>,
    pub(crate) adapter_store_path: Option<String>,
    pub(crate) created_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) ended_at: Option<String>,
    pub(crate) run_dir: String,
    pub(crate) run_path: String,
    pub(crate) metrics_path: String,
    pub(crate) raw_log_path: String,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoreImportRequest {
    pub(crate) path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetValidateRequest {
    pub(crate) path: Option<String>,
    pub(crate) dataset_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetTemplateRequestBody {
    pub(crate) task: Option<String>,
    pub(crate) language: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetExportRequest {
    pub(crate) output_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetDiffRequest {
    pub(crate) right_dataset_ref: Option<String>,
    pub(crate) right_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetSynthRequest {
    #[serde(default)]
    pub(crate) print_prompt: bool,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) output_path: Option<String>,
    pub(crate) brief: Option<String>,
    pub(crate) spec_content: Option<String>,
    pub(crate) spec_path: Option<String>,
    pub(crate) split: Option<String>,
    pub(crate) count: Option<u32>,
    pub(crate) train_count: Option<u32>,
    pub(crate) valid_count: Option<u32>,
    pub(crate) test_count: Option<u32>,
    pub(crate) eval_count: Option<u32>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) temperature: Option<f32>,
    pub(crate) timeout_seconds: Option<f32>,
    pub(crate) retries: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatasetEvalRequest {
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) output_path: Option<String>,
    pub(crate) dataset_ref: Option<String>,
    pub(crate) input_content: Option<String>,
    pub(crate) input_format: Option<String>,
    pub(crate) input_path: Option<String>,
    pub(crate) split: Option<String>,
    pub(crate) max_records: Option<u32>,
    pub(crate) criteria: Option<String>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) temperature: Option<f32>,
    pub(crate) timeout_seconds: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdapterImportRequest {
    pub(crate) path: String,
    pub(crate) base_model_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StorePullRequest {
    pub(crate) repo_id: String,
    pub(crate) revision: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdapterPullRequest {
    pub(crate) repo_id: String,
    pub(crate) revision: Option<String>,
    pub(crate) base_model_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdapterBindRequest {
    pub(crate) base_model_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrainPlanRequest {
    pub(crate) model_ref: String,
    pub(crate) dataset_ref: String,
    pub(crate) name: Option<String>,
    pub(crate) backend: Option<TrainPlanBackendRequest>,
    pub(crate) overrides: Option<TrainPlanOverridesRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrainRunStartRequest {}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TrainPlanBackendRequest {
    Auto,
    Mlx,
    Peft,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrainPlanOverridesRequest {
    pub(crate) max_seq_length: Option<u32>,
    pub(crate) mask_prompt: Option<bool>,
    pub(crate) rank: Option<u32>,
    pub(crate) learning_rate: Option<f64>,
    pub(crate) batch_size: Option<u32>,
    pub(crate) gradient_accumulation_steps: Option<u32>,
    pub(crate) max_steps: Option<u32>,
    pub(crate) seed: Option<u64>,
    pub(crate) mlx_num_layers: Option<u32>,
    pub(crate) mlx_grad_checkpoint: Option<bool>,
    pub(crate) peft_load_in_4bit: Option<bool>,
    pub(crate) peft_load_in_8bit: Option<bool>,
}
