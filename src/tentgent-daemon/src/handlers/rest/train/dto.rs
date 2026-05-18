use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{to_value, Value};
use tentgent_kernel::features::train::domain::{
    LoraTrainBackendRequest, LoraTrainMetricsTail, LoraTrainOverrides, LoraTrainPlan,
    LoraTrainPlanCreateOutcome, LoraTrainPlanInspection, LoraTrainPlanPreviewOutcome,
    LoraTrainPlanRemovalOutcome, LoraTrainPlanSummary, LoraTrainRunInspection, LoraTrainRunStatus,
    TrainRunLogMetadata, TrainRunLogTail,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainPlanRequest {
    pub model_ref: String,
    pub dataset_ref: String,
    pub name: Option<String>,
    pub backend: Option<LoraTrainBackendRequest>,
    pub overrides: Option<TrainPlanOverridesRequest>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainPlanOverridesRequest {
    pub max_seq_length: Option<u32>,
    pub mask_prompt: Option<bool>,
    pub rank: Option<u32>,
    pub learning_rate: Option<f64>,
    pub batch_size: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
    pub max_steps: Option<u32>,
    pub seed: Option<u64>,
    pub mlx_num_layers: Option<u32>,
    pub mlx_grad_checkpoint: Option<bool>,
    pub peft_load_in_4bit: Option<bool>,
    pub peft_load_in_8bit: Option<bool>,
}

impl From<TrainPlanOverridesRequest> for LoraTrainOverrides {
    fn from(value: TrainPlanOverridesRequest) -> Self {
        Self {
            max_seq_length: value.max_seq_length,
            mask_prompt: value.mask_prompt,
            rank: value.rank,
            learning_rate: value.learning_rate,
            batch_size: value.batch_size,
            gradient_accumulation_steps: value.gradient_accumulation_steps,
            max_steps: value.max_steps,
            seed: value.seed,
            mlx_num_layers: value.mlx_num_layers,
            mlx_grad_checkpoint: value.mlx_grad_checkpoint,
            peft_load_in_4bit: value.peft_load_in_4bit,
            peft_load_in_8bit: value.peft_load_in_8bit,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TrainPlansResponse {
    pub plans: Vec<TrainPlanSummaryItem>,
}

#[derive(Debug, Serialize)]
pub struct TrainPlanPreviewResponse {
    pub plan: TrainPlanItem,
    pub preview: TrainPlanPreviewItem,
}

#[derive(Debug, Serialize)]
pub struct TrainPlanCreateResponse {
    pub plan: TrainPlanItem,
    pub created: bool,
    pub deduplicated: bool,
    pub run_count: usize,
    pub plan_dir: String,
    pub plan_path: String,
}

#[derive(Debug, Serialize)]
pub struct TrainPlanResponse {
    pub plan: TrainPlanItem,
    pub run_count: usize,
    pub plan_dir: String,
    pub plan_path: String,
    pub runs_dir: String,
}

#[derive(Debug, Serialize)]
pub struct RemoveTrainPlanResponse {
    pub removed: RemovedTrainPlanItem,
    pub plan: TrainPlanItem,
}

#[derive(Debug, Serialize)]
pub struct RemovedTrainPlanItem {
    pub kind: &'static str,
    pub plan_ref: String,
    pub short_ref: String,
    pub plan_dir: String,
    pub run_count: usize,
}

#[derive(Debug, Serialize)]
pub struct TrainPlanPreviewItem {
    pub would_reuse: bool,
    pub persisted: bool,
    pub run_count: usize,
    pub would_plan_dir: String,
    pub would_plan_path: String,
}

#[derive(Debug, Serialize)]
pub struct TrainPlanSummaryItem {
    pub plan_ref: String,
    pub short_ref: String,
    pub name: Option<String>,
    pub status: String,
    pub requested_backend: String,
    pub backend: Option<String>,
    pub model_ref: String,
    pub dataset_ref: String,
    pub created_at: String,
    pub run_count: usize,
    pub plan_dir: String,
    pub plan_path: String,
}

#[derive(Debug, Serialize)]
pub struct TrainPlanItem {
    pub schema_version: u32,
    pub plan_ref: String,
    pub short_ref: String,
    pub name: Option<String>,
    pub status: String,
    pub created_at: String,
    pub model_ref: String,
    pub model_short_ref: String,
    pub dataset_ref: String,
    pub dataset_short_ref: String,
    pub requested_backend: String,
    pub backend: Option<String>,
    pub profile: String,
    pub selection_reason: String,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub model: Value,
    pub dataset: Value,
    pub lora: Value,
    pub optimization: Value,
    pub checkpoint: Value,
    pub output: Value,
    pub backend_config: Value,
    pub command_hint: String,
}

#[derive(Debug, Serialize)]
pub struct TrainRunsResponse {
    pub runs: Vec<TrainRunSummaryItem>,
}

#[derive(Debug, Serialize)]
pub struct TrainRunResponse {
    pub run: TrainRunItem,
}

#[derive(Debug, Serialize)]
pub struct TrainRunMetricsResponse {
    pub run: TrainRunRefItem,
    pub metrics_path: String,
    pub tail: usize,
    pub total_events: usize,
    pub truncated: bool,
    pub events: Vec<TrainRunMetricItem>,
    pub warnings: Vec<TrainRunWarningItem>,
}

#[derive(Debug, Serialize)]
pub struct TrainRunLogsResponse {
    pub logs: TrainRunLogsItem,
}

#[derive(Debug, Serialize)]
pub struct TrainRunLogsItem {
    pub raw: TrainRunLogMetadataItem,
}

#[derive(Debug, Serialize)]
pub struct TrainRunLogResponse {
    pub log: TrainRunLogContentItem,
}

#[derive(Debug, Serialize)]
pub struct TrainRunLogMetadataItem {
    pub kind: &'static str,
    pub path: String,
    pub exists: bool,
    pub total_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TrainRunLogContentItem {
    pub owner: &'static str,
    pub run_ref: String,
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

#[derive(Debug, Serialize)]
pub struct TrainRunRefItem {
    pub run_ref: String,
    pub short_ref: String,
}

#[derive(Debug, Serialize)]
pub struct TrainRunMetricItem {
    pub index: usize,
    pub event: Value,
}

#[derive(Debug, Serialize)]
pub struct TrainRunWarningItem {
    pub code: String,
    pub message: String,
    pub line: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct TrainRunSummaryItem {
    pub run_ref: String,
    pub short_ref: String,
    pub status: String,
    pub process_running: bool,
    pub stale: bool,
    pub phase: Option<String>,
    pub error: Option<String>,
    pub plan_ref: String,
    pub model_ref: String,
    pub dataset_ref: String,
    pub backend: Option<String>,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub adapter_ref: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub run_dir: String,
    pub run_path: String,
}

#[derive(Debug, Serialize)]
pub struct TrainRunItem {
    pub run_ref: String,
    pub short_ref: String,
    pub status: String,
    pub process_running: bool,
    pub stale: bool,
    pub phase: Option<String>,
    pub error: Option<String>,
    pub plan_ref: String,
    pub plan_short_ref: String,
    pub model_ref: String,
    pub dataset_ref: String,
    pub backend: Option<String>,
    pub recipe_hash: String,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<String>,
    pub adapter_ref: Option<String>,
    pub adapter_path: Option<String>,
    pub adapter_output_path: Option<String>,
    pub adapter_store_path: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub run_dir: String,
    pub run_path: String,
    pub metrics_path: String,
    pub raw_log_path: String,
}

pub fn train_plan_preview_response(
    outcome: LoraTrainPlanPreviewOutcome,
) -> TrainPlanPreviewResponse {
    TrainPlanPreviewResponse {
        plan: train_plan_item(&outcome.plan),
        preview: TrainPlanPreviewItem {
            would_reuse: outcome.would_reuse,
            persisted: false,
            run_count: outcome.run_count,
            would_plan_dir: path_string(&outcome.plan_dir),
            would_plan_path: path_string(&outcome.plan_path),
        },
    }
}

pub fn train_plan_create_response(outcome: LoraTrainPlanCreateOutcome) -> TrainPlanCreateResponse {
    TrainPlanCreateResponse {
        plan: train_plan_item(&outcome.plan),
        created: !outcome.deduplicated,
        deduplicated: outcome.deduplicated,
        run_count: outcome.run_count,
        plan_dir: path_string(&outcome.plan_dir),
        plan_path: path_string(&outcome.plan_path),
    }
}

pub fn train_plan_response(inspection: LoraTrainPlanInspection) -> TrainPlanResponse {
    TrainPlanResponse {
        plan: train_plan_item(&inspection.plan),
        run_count: inspection.run_count,
        plan_dir: path_string(&inspection.plan_dir),
        plan_path: path_string(&inspection.plan_path),
        runs_dir: path_string(&inspection.runs_dir),
    }
}

pub fn remove_train_plan_response(outcome: LoraTrainPlanRemovalOutcome) -> RemoveTrainPlanResponse {
    let plan = train_plan_item(&outcome.plan);
    RemoveTrainPlanResponse {
        removed: RemovedTrainPlanItem {
            kind: "lora_train_plan",
            plan_ref: outcome.plan.plan_ref,
            short_ref: outcome.plan.short_ref,
            plan_dir: path_string(&outcome.plan_dir),
            run_count: outcome.run_count,
        },
        plan,
    }
}

pub fn train_plan_summary_item(
    summary: LoraTrainPlanSummary,
    inspection: LoraTrainPlanInspection,
) -> TrainPlanSummaryItem {
    TrainPlanSummaryItem {
        plan_ref: summary.plan.plan_ref,
        short_ref: summary.plan.short_ref,
        name: summary.plan.name,
        status: summary.plan.status.as_str().to_string(),
        requested_backend: summary.plan.requested_backend.as_str().to_string(),
        backend: summary
            .plan
            .backend
            .map(|backend| backend.as_str().to_string()),
        model_ref: summary.plan.model_ref,
        dataset_ref: summary.plan.dataset_ref,
        created_at: summary.plan.created_at,
        run_count: summary.run_count,
        plan_dir: path_string(&inspection.plan_dir),
        plan_path: path_string(&inspection.plan_path),
    }
}

pub fn train_plan_item(plan: &LoraTrainPlan) -> TrainPlanItem {
    TrainPlanItem {
        schema_version: plan.schema_version,
        plan_ref: plan.plan_ref.clone(),
        short_ref: plan.short_ref.clone(),
        name: plan.name.clone(),
        status: plan.status.as_str().to_string(),
        created_at: plan.created_at.clone(),
        model_ref: plan.model_ref.clone(),
        model_short_ref: plan.model_short_ref.clone(),
        dataset_ref: plan.dataset_ref.clone(),
        dataset_short_ref: plan.dataset_short_ref.clone(),
        requested_backend: plan.requested_backend.as_str().to_string(),
        backend: plan.backend.map(|backend| backend.as_str().to_string()),
        profile: plan.profile.clone(),
        selection_reason: plan.selection_reason.clone(),
        blockers: plan.blockers.clone(),
        warnings: plan.warnings.clone(),
        model: value_or_null(&plan.model),
        dataset: value_or_null(&plan.dataset),
        lora: value_or_null(&plan.lora),
        optimization: value_or_null(&plan.optimization),
        checkpoint: value_or_null(&plan.checkpoint),
        output: value_or_null(&plan.output),
        backend_config: value_or_null(&plan.backend_config),
        command_hint: plan.command_hint.clone(),
    }
}

pub fn train_run_summary_item(inspection: LoraTrainRunInspection) -> TrainRunSummaryItem {
    let run = inspection.run;
    TrainRunSummaryItem {
        run_ref: run.run_ref,
        short_ref: run.short_ref,
        status: effective_run_status(run.status, inspection.stale),
        process_running: inspection.process_running,
        stale: inspection.stale,
        phase: run.phase,
        error: run.error.or_else(|| stale_error(inspection.stale)),
        plan_ref: run.plan_ref,
        model_ref: run.model_ref,
        dataset_ref: run.dataset_ref,
        backend: run.backend.map(|backend| backend.as_str().to_string()),
        pid: run.pid,
        exit_code: run.exit_code,
        adapter_ref: run.adapter_ref,
        created_at: run.created_at,
        started_at: run.started_at,
        ended_at: run.ended_at,
        run_dir: path_string(&inspection.run_dir),
        run_path: path_string(&inspection.run_path),
    }
}

pub fn train_run_item(inspection: LoraTrainRunInspection) -> TrainRunItem {
    let run = inspection.run;
    TrainRunItem {
        run_ref: run.run_ref,
        short_ref: run.short_ref,
        status: effective_run_status(run.status, inspection.stale),
        process_running: inspection.process_running,
        stale: inspection.stale,
        phase: run.phase,
        error: run.error.or_else(|| stale_error(inspection.stale)),
        plan_ref: run.plan_ref,
        plan_short_ref: run.plan_short_ref,
        model_ref: run.model_ref,
        dataset_ref: run.dataset_ref,
        backend: run.backend.map(|backend| backend.as_str().to_string()),
        recipe_hash: run.recipe_hash,
        pid: run.pid,
        exit_code: run.exit_code,
        exit_signal: run.exit_signal,
        adapter_ref: run.adapter_ref,
        adapter_path: run.adapter_path,
        adapter_output_path: run.adapter_output_path,
        adapter_store_path: run.adapter_store_path,
        created_at: run.created_at,
        started_at: run.started_at,
        ended_at: run.ended_at,
        run_dir: path_string(&inspection.run_dir),
        run_path: path_string(&inspection.run_path),
        metrics_path: path_string(&inspection.metrics_path),
        raw_log_path: path_string(&inspection.raw_log_path),
    }
}

pub fn train_run_metrics_response_body(
    inspection: LoraTrainRunInspection,
    metrics: LoraTrainMetricsTail,
) -> TrainRunMetricsResponse {
    TrainRunMetricsResponse {
        run: TrainRunRefItem {
            run_ref: inspection.run.run_ref,
            short_ref: inspection.run.short_ref,
        },
        metrics_path: path_string(&metrics.metrics_path),
        tail: metrics.tail,
        total_events: metrics.total_events,
        truncated: metrics.truncated,
        events: metrics
            .events
            .into_iter()
            .map(|event| TrainRunMetricItem {
                index: event.index,
                event: event.event,
            })
            .collect(),
        warnings: metrics
            .warnings
            .into_iter()
            .map(|warning| TrainRunWarningItem {
                code: warning.code,
                message: warning.message,
                line: warning.line,
            })
            .collect(),
    }
}

pub fn train_run_log_metadata_item(metadata: TrainRunLogMetadata) -> TrainRunLogMetadataItem {
    TrainRunLogMetadataItem {
        kind: "raw",
        path: path_string(&metadata.path),
        exists: metadata.exists,
        total_bytes: metadata.total_bytes,
        modified_at: metadata.modified_at,
    }
}

pub fn train_run_log_response_body(
    inspection: LoraTrainRunInspection,
    log: TrainRunLogTail,
) -> TrainRunLogResponse {
    TrainRunLogResponse {
        log: TrainRunLogContentItem {
            owner: "train_run",
            run_ref: inspection.run.run_ref,
            short_ref: inspection.run.short_ref,
            kind: "raw",
            path: path_string(&log.metadata.path),
            exists: log.metadata.exists,
            total_bytes: log.metadata.total_bytes,
            modified_at: log.metadata.modified_at,
            tail_bytes: log.tail_bytes,
            truncated: log.truncated,
            encoding: log.encoding,
            content: log.content,
        },
    }
}

fn effective_run_status(status: LoraTrainRunStatus, stale: bool) -> String {
    if stale {
        "stale".to_string()
    } else {
        status.as_str().to_string()
    }
}

fn stale_error(stale: bool) -> Option<String> {
    stale
        .then(|| "run process is no longer running but no terminal status was recorded".to_string())
}

fn value_or_null(value: &impl serde::Serialize) -> Value {
    to_value(value).unwrap_or(Value::Null)
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
