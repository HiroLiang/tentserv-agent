use serde_json::{to_value, Value};
use tentgent_core::{
    dataset::DatasetError,
    model::ModelError,
    train::{
        launch_detached_lora_run_worker, LoraTrainBackendRequest, LoraTrainMetricsTail,
        LoraTrainOverrides, LoraTrainPlan, LoraTrainPlanCreateOutcome, LoraTrainPlanInspection,
        LoraTrainPlanManager, LoraTrainPlanPreviewOutcome, LoraTrainPlanSummary,
        LoraTrainRunInspection, LoraTrainRunManager, TrainError, TrainRunLogMetadata,
        TrainRunLogTail,
    },
};

use crate::{
    app::DaemonHttpState,
    dto::{
        ErrorResponse, RemoveTrainPlanResponse, RemovedTrainPlanItem, TrainPlanBackendRequest,
        TrainPlanCreateResponse, TrainPlanItem, TrainPlanPreviewItem, TrainPlanPreviewResponse,
        TrainPlanRequest, TrainPlanResponse, TrainPlanSummaryItem, TrainPlansResponse,
        TrainRunItem, TrainRunLogContentItem, TrainRunLogMetadataItem, TrainRunLogResponse,
        TrainRunLogsItem, TrainRunLogsResponse, TrainRunMetricItem, TrainRunMetricsResponse,
        TrainRunRefItem, TrainRunResponse, TrainRunStartRequest, TrainRunStartResponse,
        TrainRunSummaryItem, TrainRunWarningItem, TrainRunsResponse,
    },
    http::{HttpRequest, HttpResponse},
    response::{bad_request_response, json_response, parse_json_body},
};

use super::store::path_string;

const DEFAULT_METRICS_TAIL: usize = 200;
const MAX_METRICS_TAIL: usize = 1_000;
const DEFAULT_RAW_LOG_TAIL_BYTES: u64 = 65_536;
const MAX_RAW_LOG_TAIL_BYTES: u64 = 262_144;

pub(crate) fn list_train_plans_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match LoraTrainPlanManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_error_response(error),
    };

    let plans = match manager.list_plans() {
        Ok(plans) => plans,
        Err(error) => return train_error_response(error),
    };

    let mut items = Vec::new();
    for summary in plans {
        let inspection = match manager.inspect_plan(&summary.plan.plan_ref) {
            Ok(inspection) => inspection,
            Err(error) => return train_error_response(error),
        };
        items.push(train_plan_summary_item(summary, inspection));
    }

    items.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.plan_ref.cmp(&right.plan_ref))
    });

    json_response(200, TrainPlansResponse { plans: items })
}

pub(crate) async fn start_train_run_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    plan_reference: &str,
) -> HttpResponse {
    if let Err(response) = parse_start_run_body(request) {
        return response;
    }

    let manager = match LoraTrainRunManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_run_error_response(error),
    };
    let outcome = match manager.start_run(plan_reference) {
        Ok(outcome) => outcome,
        Err(error) => return train_run_error_response(error),
    };

    let home_dir = state.home_dir().to_path_buf();
    let run_ref = outcome.run.run_ref.clone();
    let launch =
        tokio::task::spawn_blocking(move || launch_detached_lora_run_worker(&home_dir, &run_ref))
            .await;

    let pid = match launch {
        Ok(Ok(pid)) => pid,
        Ok(Err(error)) => {
            let _ = manager.mark_run_failed(
                &outcome.run.run_ref,
                "worker_spawn",
                error.to_string(),
                None,
            );
            return train_run_error_response(error);
        }
        Err(error) => {
            let message = format!("failed to join LoRA training worker launch task: {error}");
            let _ = manager.mark_run_failed(
                &outcome.run.run_ref,
                "worker_spawn",
                message.clone(),
                None,
            );
            return json_response(
                500,
                ErrorResponse {
                    error: "train_run_failed",
                    message,
                },
            );
        }
    };

    let run = match manager.record_worker_started(&outcome.run.run_ref, pid) {
        Ok(run) => run,
        Err(error) => return train_run_error_response(error),
    };
    let inspection = match manager.inspect_run(&run.run_ref) {
        Ok(inspection) => inspection,
        Err(error) => return train_run_error_response(error),
    };

    json_response(
        202,
        TrainRunStartResponse {
            run: train_run_item(inspection),
            plan: train_plan_item(&outcome.plan),
        },
    )
}

pub(crate) fn list_train_runs_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match LoraTrainRunManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_run_error_response(error),
    };
    match manager.list_runs() {
        Ok(runs) => json_response(
            200,
            TrainRunsResponse {
                runs: runs.into_iter().map(train_run_summary_item).collect(),
            },
        ),
        Err(error) => train_run_error_response(error),
    }
}

pub(crate) fn list_plan_runs_response(
    state: &DaemonHttpState,
    plan_reference: &str,
) -> HttpResponse {
    let manager = match LoraTrainRunManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_run_error_response(error),
    };
    match manager.list_plan_runs(plan_reference) {
        Ok(runs) => json_response(
            200,
            TrainRunsResponse {
                runs: runs.into_iter().map(train_run_summary_item).collect(),
            },
        ),
        Err(error) => train_run_error_response(error),
    }
}

pub(crate) fn inspect_train_run_response(
    state: &DaemonHttpState,
    run_reference: &str,
) -> HttpResponse {
    let manager = match LoraTrainRunManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_run_error_response(error),
    };
    match manager.inspect_run(run_reference) {
        Ok(inspection) => json_response(
            200,
            TrainRunResponse {
                run: train_run_item(inspection),
            },
        ),
        Err(error) => train_run_error_response(error),
    }
}

pub(crate) fn train_run_metrics_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    run_reference: &str,
) -> HttpResponse {
    let tail = match metrics_tail(request) {
        Ok(tail) => tail,
        Err(response) => return response,
    };
    let manager = match LoraTrainRunManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_run_error_response(error),
    };
    let inspection = match manager.inspect_run(run_reference) {
        Ok(inspection) => inspection,
        Err(error) => return train_run_error_response(error),
    };
    match manager.metrics_tail(run_reference, tail) {
        Ok(metrics) => json_response(200, train_run_metrics_response_body(inspection, metrics)),
        Err(error) => train_run_error_response(error),
    }
}

pub(crate) fn train_run_logs_response(
    state: &DaemonHttpState,
    run_reference: &str,
) -> HttpResponse {
    let manager = match LoraTrainRunManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_run_error_response(error),
    };
    match manager.raw_log_metadata(run_reference) {
        Ok(metadata) => json_response(
            200,
            TrainRunLogsResponse {
                logs: TrainRunLogsItem {
                    raw: train_run_log_metadata_item(metadata),
                },
            },
        ),
        Err(error) => train_run_error_response(error),
    }
}

pub(crate) fn train_run_raw_log_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    run_reference: &str,
) -> HttpResponse {
    let tail_bytes = match raw_log_tail_bytes(request) {
        Ok(tail_bytes) => tail_bytes,
        Err(response) => return response,
    };
    let manager = match LoraTrainRunManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_run_error_response(error),
    };
    let inspection = match manager.inspect_run(run_reference) {
        Ok(inspection) => inspection,
        Err(error) => return train_run_error_response(error),
    };
    match manager.raw_log_tail(run_reference, tail_bytes) {
        Ok(log) => json_response(200, train_run_log_response_body(inspection, log)),
        Err(error) => train_run_error_response(error),
    }
}

pub(crate) fn preview_train_plan_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_train_plan_request(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let manager = match LoraTrainPlanManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_error_response(error),
    };

    match manager.preview_plan(
        &body.model_ref,
        &body.dataset_ref,
        body.backend,
        body.name,
        body.overrides,
    ) {
        Ok(outcome) => json_response(200, train_plan_preview_response(outcome)),
        Err(error) => train_error_response(error),
    }
}

pub(crate) fn create_train_plan_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_train_plan_request(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let manager = match LoraTrainPlanManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_error_response(error),
    };

    match manager.create_plan(
        &body.model_ref,
        &body.dataset_ref,
        body.backend,
        body.name,
        body.overrides,
    ) {
        Ok(outcome) => json_response(200, train_plan_create_response(outcome)),
        Err(error) => train_error_response(error),
    }
}

pub(crate) fn inspect_train_plan_response(
    state: &DaemonHttpState,
    reference: &str,
) -> HttpResponse {
    let manager = match LoraTrainPlanManager::open_readonly_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_error_response(error),
    };

    match manager.inspect_plan(reference) {
        Ok(inspection) => json_response(200, train_plan_response(inspection)),
        Err(error) => train_error_response(error),
    }
}

pub(crate) fn remove_train_plan_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    if !request.body.is_empty() {
        return bad_request_response("DELETE requests for train plans must not include a body");
    }

    let manager = match LoraTrainPlanManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return train_error_response(error),
    };
    let inspection = match manager.inspect_plan(reference) {
        Ok(inspection) => inspection,
        Err(error) => return train_error_response(error),
    };

    if inspection.run_count > 0 {
        return json_response(
            409,
            ErrorResponse {
                error: "in_use",
                message: format!(
                    "LoRA train plan `{}` has {} run record(s); remove runs before deleting the plan",
                    inspection.plan.short_ref, inspection.run_count
                ),
            },
        );
    }

    let plan = train_plan_item(&inspection.plan);
    match manager.remove_plan(reference) {
        Ok(outcome) => json_response(
            200,
            RemoveTrainPlanResponse {
                removed: RemovedTrainPlanItem {
                    kind: "lora_train_plan",
                    plan_ref: outcome.plan.plan_ref,
                    short_ref: outcome.plan.short_ref,
                    plan_dir: path_string(&outcome.plan_dir),
                    run_count: outcome.run_count,
                },
                plan,
            },
        ),
        Err(error) => train_error_response(error),
    }
}

struct ParsedTrainPlanRequest {
    model_ref: String,
    dataset_ref: String,
    name: Option<String>,
    backend: LoraTrainBackendRequest,
    overrides: LoraTrainOverrides,
}

fn parse_train_plan_request(request: &HttpRequest) -> Result<ParsedTrainPlanRequest, HttpResponse> {
    let body = parse_json_body::<TrainPlanRequest>(request)?;
    let model_ref = normalize_required_ref(body.model_ref, "model_ref")?;
    let dataset_ref = normalize_required_ref(body.dataset_ref, "dataset_ref")?;
    let name = normalize_optional_display_name(body.name);
    let backend = body
        .backend
        .map(Into::into)
        .unwrap_or(LoraTrainBackendRequest::Auto);
    let overrides = validate_overrides(body.overrides.unwrap_or_default())?;

    Ok(ParsedTrainPlanRequest {
        model_ref,
        dataset_ref,
        name,
        backend,
        overrides,
    })
}

fn parse_start_run_body(request: &HttpRequest) -> Result<(), HttpResponse> {
    if request.body.is_empty() {
        return Ok(());
    }
    parse_json_body::<TrainRunStartRequest>(request).map(|_| ())
}

fn normalize_required_ref(value: String, field: &'static str) -> Result<String, HttpResponse> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(bad_request_response(format!("{field} must not be blank")));
    }
    if trimmed.contains('/') {
        return Err(bad_request_response(format!(
            "{field} must be a managed ref, not a path"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_display_name(value: Option<String>) -> Option<String> {
    value.and_then(|name| {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn validate_overrides(
    overrides: crate::dto::TrainPlanOverridesRequest,
) -> Result<LoraTrainOverrides, HttpResponse> {
    validate_positive_u32(overrides.max_seq_length, "overrides.max_seq_length")?;
    validate_positive_u32(overrides.rank, "overrides.rank")?;
    validate_positive_f64(overrides.learning_rate, "overrides.learning_rate")?;
    validate_positive_u32(overrides.batch_size, "overrides.batch_size")?;
    validate_positive_u32(
        overrides.gradient_accumulation_steps,
        "overrides.gradient_accumulation_steps",
    )?;
    validate_positive_u32(overrides.max_steps, "overrides.max_steps")?;
    validate_positive_u32(overrides.mlx_num_layers, "overrides.mlx_num_layers")?;

    if overrides.peft_load_in_4bit == Some(true) && overrides.peft_load_in_8bit == Some(true) {
        return Err(bad_request_response(
            "overrides.peft_load_in_4bit and overrides.peft_load_in_8bit cannot both be true",
        ));
    }

    Ok(LoraTrainOverrides {
        max_seq_length: overrides.max_seq_length,
        mask_prompt: overrides.mask_prompt,
        rank: overrides.rank,
        learning_rate: overrides.learning_rate,
        batch_size: overrides.batch_size,
        gradient_accumulation_steps: overrides.gradient_accumulation_steps,
        max_steps: overrides.max_steps,
        seed: overrides.seed,
        mlx_num_layers: overrides.mlx_num_layers,
        mlx_grad_checkpoint: overrides.mlx_grad_checkpoint,
        peft_load_in_4bit: overrides.peft_load_in_4bit,
        peft_load_in_8bit: overrides.peft_load_in_8bit,
    })
}

fn validate_positive_u32(value: Option<u32>, field: &'static str) -> Result<(), HttpResponse> {
    if value == Some(0) {
        return Err(bad_request_response(format!(
            "{field} must be greater than 0"
        )));
    }
    Ok(())
}

fn validate_positive_f64(value: Option<f64>, field: &'static str) -> Result<(), HttpResponse> {
    if let Some(value) = value {
        if !value.is_finite() || value <= 0.0 {
            return Err(bad_request_response(format!(
                "{field} must be greater than 0"
            )));
        }
    }
    Ok(())
}

impl From<TrainPlanBackendRequest> for LoraTrainBackendRequest {
    fn from(value: TrainPlanBackendRequest) -> Self {
        match value {
            TrainPlanBackendRequest::Auto => Self::Auto,
            TrainPlanBackendRequest::Mlx => Self::Mlx,
            TrainPlanBackendRequest::Peft => Self::Peft,
        }
    }
}

fn train_plan_preview_response(outcome: LoraTrainPlanPreviewOutcome) -> TrainPlanPreviewResponse {
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

fn train_plan_create_response(outcome: LoraTrainPlanCreateOutcome) -> TrainPlanCreateResponse {
    let created = !outcome.deduplicated;
    TrainPlanCreateResponse {
        plan: train_plan_item(&outcome.plan),
        created,
        deduplicated: outcome.deduplicated,
        run_count: outcome.run_count,
        plan_dir: path_string(&outcome.plan_dir),
        plan_path: path_string(&outcome.plan_path),
    }
}

fn train_plan_response(inspection: LoraTrainPlanInspection) -> TrainPlanResponse {
    TrainPlanResponse {
        plan: train_plan_item(&inspection.plan),
        run_count: inspection.run_count,
        plan_dir: path_string(&inspection.plan_dir),
        plan_path: path_string(&inspection.plan_path),
        runs_dir: path_string(&inspection.runs_dir),
    }
}

fn train_plan_summary_item(
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

fn train_plan_item(plan: &LoraTrainPlan) -> TrainPlanItem {
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

fn train_run_summary_item(inspection: LoraTrainRunInspection) -> TrainRunSummaryItem {
    let run = inspection.run;
    TrainRunSummaryItem {
        run_ref: run.run_ref,
        short_ref: run.short_ref,
        status: effective_run_status(&run.status, inspection.stale),
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

fn train_run_item(inspection: LoraTrainRunInspection) -> TrainRunItem {
    let run = inspection.run;
    TrainRunItem {
        run_ref: run.run_ref,
        short_ref: run.short_ref,
        status: effective_run_status(&run.status, inspection.stale),
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

fn train_run_metrics_response_body(
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

fn train_run_log_response_body(
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

fn train_run_log_metadata_item(metadata: TrainRunLogMetadata) -> TrainRunLogMetadataItem {
    TrainRunLogMetadataItem {
        kind: "raw",
        path: path_string(&metadata.path),
        exists: metadata.exists,
        total_bytes: metadata.total_bytes,
        modified_at: metadata.modified_at,
    }
}

fn effective_run_status(status: &tentgent_core::train::LoraTrainRunStatus, stale: bool) -> String {
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

fn metrics_tail(request: &HttpRequest) -> Result<usize, HttpResponse> {
    let values = request.query_values("tail").collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(DEFAULT_METRICS_TAIL),
        [value] => parse_metrics_tail(value),
        _ => Err(bad_request_response("`tail` must be provided at most once")),
    }
}

fn parse_metrics_tail(value: &str) -> Result<usize, HttpResponse> {
    let parsed = value.parse::<usize>().map_err(|_| {
        bad_request_response(format!(
            "`tail` must be an integer between 1 and {MAX_METRICS_TAIL}"
        ))
    })?;
    if parsed == 0 || parsed > MAX_METRICS_TAIL {
        return Err(bad_request_response(format!(
            "`tail` must be between 1 and {MAX_METRICS_TAIL}"
        )));
    }
    Ok(parsed)
}

fn raw_log_tail_bytes(request: &HttpRequest) -> Result<u64, HttpResponse> {
    let values = request.query_values("tail_bytes").collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(DEFAULT_RAW_LOG_TAIL_BYTES),
        [value] => parse_raw_log_tail_bytes(value),
        _ => Err(bad_request_response(
            "`tail_bytes` must be provided at most once",
        )),
    }
}

fn parse_raw_log_tail_bytes(value: &str) -> Result<u64, HttpResponse> {
    let parsed = value.parse::<u64>().map_err(|_| {
        bad_request_response(format!(
            "`tail_bytes` must be an integer between 1 and {MAX_RAW_LOG_TAIL_BYTES}"
        ))
    })?;
    if parsed == 0 || parsed > MAX_RAW_LOG_TAIL_BYTES {
        return Err(bad_request_response(format!(
            "`tail_bytes` must be between 1 and {MAX_RAW_LOG_TAIL_BYTES}"
        )));
    }
    Ok(parsed)
}

fn value_or_null(value: &impl serde::Serialize) -> Value {
    to_value(value).unwrap_or(Value::Null)
}

fn train_error_response(error: TrainError) -> HttpResponse {
    match error {
        TrainError::Model(ModelError::NotFound(reference))
        | TrainError::Dataset(DatasetError::NotFound(reference))
        | TrainError::PlanNotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("training reference `{reference}` was not found"),
            },
        ),
        TrainError::Model(ModelError::AmbiguousRef(reference))
        | TrainError::Dataset(DatasetError::AmbiguousRef(reference))
        | TrainError::AmbiguousPlanRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "training reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        other => json_response(
            500,
            ErrorResponse {
                error: "train_plan_failed",
                message: format!("failed to manage LoRA train plans: {other}"),
            },
        ),
    }
}

fn train_run_error_response(error: TrainError) -> HttpResponse {
    match error {
        TrainError::Model(ModelError::NotFound(reference))
        | TrainError::Dataset(DatasetError::NotFound(reference))
        | TrainError::PlanNotFound(reference)
        | TrainError::RunNotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("training reference `{reference}` was not found"),
            },
        ),
        TrainError::Model(ModelError::AmbiguousRef(reference))
        | TrainError::Dataset(DatasetError::AmbiguousRef(reference))
        | TrainError::AmbiguousPlanRef(reference)
        | TrainError::AmbiguousRunRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "training reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        TrainError::PlanBlocked { plan_ref, reasons } => json_response(
            409,
            ErrorResponse {
                error: "plan_blocked",
                message: format!("LoRA train plan `{plan_ref}` is blocked: {reasons}"),
            },
        ),
        TrainError::RunAlreadyRunning(run_ref) => json_response(
            409,
            ErrorResponse {
                error: "run_already_running",
                message: format!("another LoRA train run is already running: {run_ref}"),
            },
        ),
        other => json_response(
            500,
            ErrorResponse {
                error: "train_run_failed",
                message: format!("failed to manage LoRA train runs: {other}"),
            },
        ),
    }
}
