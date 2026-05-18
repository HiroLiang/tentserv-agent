mod dto;

use std::time::Duration;

use axum::{
    extract::{Path, RawQuery, State},
    http::StatusCode,
    Json,
};
use tentgent_kernel::{
    features::{
        dataset::domain::DatasetRefSelector,
        model::domain::ModelRefSelector,
        train::{
            domain::{LoraTrainBackendRequest, LoraTrainRunStatus, TrainRefSelector},
            ports::LoraTrainWorkerLauncher,
            usecases::{
                LoraTrainMetricsTailRequest, LoraTrainPlanBuildRequest,
                LoraTrainPlanInspectRequest, LoraTrainPlanListRequest, LoraTrainPlanRemoveRequest,
                LoraTrainPlanUseCase, LoraTrainRawLogMetadataRequest, LoraTrainRawLogTailRequest,
                LoraTrainRunInspectRequest, LoraTrainRunListRequest, LoraTrainRunMarkFailedRequest,
                LoraTrainRunStartRequest, LoraTrainRunUseCase, LoraTrainRunWorkerStartedRequest,
            },
        },
    },
    foundation::{
        error::KernelError,
        layout::{LayoutResolveMode, RuntimeLayoutInput},
    },
};

use self::dto::{
    remove_train_plan_response, train_plan_create_response, train_plan_preview_response,
    train_plan_response, train_plan_summary_item, train_run_item, train_run_log_metadata_item,
    train_run_log_response_body, train_run_metrics_response_body, train_run_summary_item,
    RemoveTrainPlanResponse, TrainPlanCreateResponse, TrainPlanPreviewResponse, TrainPlanRequest,
    TrainPlanResponse, TrainPlansResponse, TrainRunLogsItem, TrainRunLogsResponse,
    TrainRunMetricsResponse, TrainRunResponse, TrainRunsResponse,
};

use crate::{
    handlers::rest::jobs::{job_item, JobResponse},
    runtime::{
        JobArtifact, JobCompletion, JobId, JobKind, JobOutputLine, JobProgressUpdate, JobRegistry,
        JobStream, JobTarget,
    },
    transport::rest::{error::RestError, state::RestState},
};

const DEFAULT_METRICS_TAIL: usize = 200;
const MAX_METRICS_TAIL: usize = 1_000;
const DEFAULT_RAW_LOG_TAIL_BYTES: u64 = 65_536;
const MAX_RAW_LOG_TAIL_BYTES: u64 = 262_144;
const TRAIN_RUN_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub async fn list_plans(
    State(state): State<RestState>,
) -> Result<Json<TrainPlansResponse>, RestError> {
    let layout = state.app().layout_input(LayoutResolveMode::ReadOnly);
    let result = state
        .app()
        .services()
        .kernel()
        .train_plan_usecase()
        .list_plans(LoraTrainPlanListRequest {
            layout: layout.clone(),
        })
        .map_err(train_plan_error)?;

    let mut plans = Vec::new();
    for summary in result.plans {
        let inspection = state
            .app()
            .services()
            .kernel()
            .train_plan_usecase()
            .inspect_plan(LoraTrainPlanInspectRequest {
                layout: layout.clone(),
                selector: TrainRefSelector::parse(&summary.plan.plan_ref).map_err(|err| {
                    RestError::bad_request(
                        "bad_request",
                        format!("invalid train plan reference: {err}"),
                    )
                })?,
            })
            .map_err(train_plan_error)?;
        plans.push(train_plan_summary_item(summary, inspection.inspection));
    }

    plans.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.plan_ref.cmp(&right.plan_ref))
    });

    Ok(Json(TrainPlansResponse { plans }))
}

pub async fn preview_plan(
    State(state): State<RestState>,
    Json(request): Json<TrainPlanRequest>,
) -> Result<Json<TrainPlanPreviewResponse>, RestError> {
    let request =
        parsed_plan_request(state.app().layout_input(LayoutResolveMode::Create), request)?;
    let outcome = state
        .app()
        .services()
        .kernel()
        .train_plan_usecase()
        .preview_plan(request)
        .map_err(train_plan_error)?;

    Ok(Json(train_plan_preview_response(outcome)))
}

pub async fn create_plan(
    State(state): State<RestState>,
    Json(request): Json<TrainPlanRequest>,
) -> Result<Json<TrainPlanCreateResponse>, RestError> {
    let request =
        parsed_plan_request(state.app().layout_input(LayoutResolveMode::Create), request)?;
    let outcome = state
        .app()
        .services()
        .kernel()
        .train_plan_usecase()
        .create_plan(request)
        .map_err(train_plan_error)?;

    Ok(Json(train_plan_create_response(outcome)))
}

pub async fn inspect_plan(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<TrainPlanResponse>, RestError> {
    let outcome = state
        .app()
        .services()
        .kernel()
        .train_plan_usecase()
        .inspect_plan(LoraTrainPlanInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector: parse_train_ref(&reference, "train plan reference")?,
        })
        .map_err(train_plan_error)?;

    Ok(Json(train_plan_response(outcome.inspection)))
}

pub async fn remove_plan(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<RemoveTrainPlanResponse>, RestError> {
    let selector = parse_train_ref(&reference, "train plan reference")?;
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let inspection = state
        .app()
        .services()
        .kernel()
        .train_plan_usecase()
        .inspect_plan(LoraTrainPlanInspectRequest {
            layout: layout.clone(),
            selector: selector.clone(),
        })
        .map_err(train_plan_error)?;
    if inspection.inspection.run_count > 0 {
        return Err(RestError::conflict(
            "in_use",
            format!(
                "LoRA train plan `{}` has {} run record(s); remove runs before deleting the plan",
                inspection.inspection.plan.short_ref, inspection.inspection.run_count
            ),
        ));
    }

    let outcome = state
        .app()
        .services()
        .kernel()
        .train_plan_usecase()
        .remove_plan(LoraTrainPlanRemoveRequest { layout, selector })
        .map_err(train_plan_error)?;

    Ok(Json(remove_train_plan_response(outcome.outcome)))
}

pub async fn list_runs(
    State(state): State<RestState>,
) -> Result<Json<TrainRunsResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .list_runs(LoraTrainRunListRequest::All {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(train_run_error)?;

    Ok(Json(TrainRunsResponse {
        runs: result
            .runs
            .into_iter()
            .map(train_run_summary_item)
            .collect(),
    }))
}

pub async fn list_plan_runs(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<TrainRunsResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .list_runs(LoraTrainRunListRequest::Plan {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            plan_selector: parse_train_ref(&reference, "train plan reference")?,
        })
        .map_err(train_run_error)?;

    Ok(Json(TrainRunsResponse {
        runs: result
            .runs
            .into_iter()
            .map(train_run_summary_item)
            .collect(),
    }))
}

pub async fn inspect_run(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<TrainRunResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .inspect_run(LoraTrainRunInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            run_selector: parse_train_ref(&reference, "train run reference")?,
        })
        .map_err(train_run_error)?;

    Ok(Json(TrainRunResponse {
        run: train_run_item(result.inspection),
    }))
}

pub async fn metrics(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<TrainRunMetricsResponse>, RestError> {
    let tail = metrics_tail(query.as_deref())?;
    let selector = parse_train_ref(&reference, "train run reference")?;
    let layout = state.app().layout_input(LayoutResolveMode::ReadOnly);
    let inspection = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .inspect_run(LoraTrainRunInspectRequest {
            layout: layout.clone(),
            run_selector: selector.clone(),
        })
        .map_err(train_run_error)?;
    let metrics = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .metrics_tail(LoraTrainMetricsTailRequest {
            layout,
            run_selector: selector,
            tail,
        })
        .map_err(train_run_error)?;

    Ok(Json(train_run_metrics_response_body(
        inspection.inspection,
        metrics,
    )))
}

pub async fn logs(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<TrainRunLogsResponse>, RestError> {
    let metadata = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .raw_log_metadata(LoraTrainRawLogMetadataRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            run_selector: parse_train_ref(&reference, "train run reference")?,
        })
        .map_err(train_run_error)?;

    Ok(Json(TrainRunLogsResponse {
        logs: TrainRunLogsItem {
            raw: train_run_log_metadata_item(metadata),
        },
    }))
}

pub async fn raw_log(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<dto::TrainRunLogResponse>, RestError> {
    let tail_bytes = raw_log_tail_bytes(query.as_deref())?;
    let selector = parse_train_ref(&reference, "train run reference")?;
    let layout = state.app().layout_input(LayoutResolveMode::ReadOnly);
    let inspection = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .inspect_run(LoraTrainRunInspectRequest {
            layout: layout.clone(),
            run_selector: selector.clone(),
        })
        .map_err(train_run_error)?;
    let log = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .raw_log_tail(LoraTrainRawLogTailRequest {
            layout,
            run_selector: selector,
            tail_bytes,
        })
        .map_err(train_run_error)?;

    Ok(Json(train_run_log_response_body(
        inspection.inspection,
        log,
    )))
}

fn parsed_plan_request(
    layout: RuntimeLayoutInput,
    request: TrainPlanRequest,
) -> Result<LoraTrainPlanBuildRequest, RestError> {
    validate_overrides(&request.overrides)?;
    Ok(LoraTrainPlanBuildRequest {
        layout,
        model_selector: parse_model_ref(&request.model_ref)?,
        dataset_selector: parse_dataset_ref(&request.dataset_ref)?,
        requested_backend: request.backend.unwrap_or(LoraTrainBackendRequest::Auto),
        name: normalize_optional_display_name(request.name),
        overrides: request.overrides.unwrap_or_default().into(),
    })
}

fn parse_model_ref(value: &str) -> Result<ModelRefSelector, RestError> {
    reject_path_ref(value, "model_ref")?;
    ModelRefSelector::parse(value)
        .map_err(|err| RestError::bad_request("bad_request", format!("invalid `model_ref`: {err}")))
}

fn parse_dataset_ref(value: &str) -> Result<DatasetRefSelector, RestError> {
    reject_path_ref(value, "dataset_ref")?;
    DatasetRefSelector::parse(value).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid `dataset_ref`: {err}"))
    })
}

fn parse_train_ref(value: &str, label: &'static str) -> Result<TrainRefSelector, RestError> {
    TrainRefSelector::parse(value)
        .map_err(|err| RestError::bad_request("bad_request", format!("invalid {label}: {err}")))
}

fn reject_path_ref(value: &str, field: &'static str) -> Result<(), RestError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RestError::bad_request(
            "bad_request",
            format!("{field} must not be blank"),
        ));
    }
    if trimmed.contains('/') {
        return Err(RestError::bad_request(
            "bad_request",
            format!("{field} must be a managed ref, not a path"),
        ));
    }
    Ok(())
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

fn validate_overrides(overrides: &Option<dto::TrainPlanOverridesRequest>) -> Result<(), RestError> {
    let Some(overrides) = overrides else {
        return Ok(());
    };
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
        return Err(RestError::bad_request(
            "bad_request",
            "overrides.peft_load_in_4bit and overrides.peft_load_in_8bit cannot both be true",
        ));
    }
    Ok(())
}

fn validate_positive_u32(value: Option<u32>, field: &'static str) -> Result<(), RestError> {
    if value == Some(0) {
        return Err(RestError::bad_request(
            "bad_request",
            format!("{field} must be greater than 0"),
        ));
    }
    Ok(())
}

fn validate_positive_f64(value: Option<f64>, field: &'static str) -> Result<(), RestError> {
    if let Some(value) = value {
        if !value.is_finite() || value <= 0.0 {
            return Err(RestError::bad_request(
                "bad_request",
                format!("{field} must be greater than 0"),
            ));
        }
    }
    Ok(())
}

fn metrics_tail(query: Option<&str>) -> Result<usize, RestError> {
    let values = query_values(query, "tail");
    match values.as_slice() {
        [] => Ok(DEFAULT_METRICS_TAIL),
        [value] => parse_metrics_tail(value),
        _ => Err(RestError::bad_request(
            "bad_request",
            "`tail` must be provided at most once",
        )),
    }
}

fn parse_metrics_tail(value: &str) -> Result<usize, RestError> {
    let parsed = value.parse::<usize>().map_err(|_| {
        RestError::bad_request(
            "bad_request",
            format!("`tail` must be an integer between 1 and {MAX_METRICS_TAIL}"),
        )
    })?;
    if parsed == 0 || parsed > MAX_METRICS_TAIL {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`tail` must be between 1 and {MAX_METRICS_TAIL}"),
        ));
    }
    Ok(parsed)
}

fn raw_log_tail_bytes(query: Option<&str>) -> Result<u64, RestError> {
    let values = query_values(query, "tail_bytes");
    match values.as_slice() {
        [] => Ok(DEFAULT_RAW_LOG_TAIL_BYTES),
        [value] => parse_raw_log_tail_bytes(value),
        _ => Err(RestError::bad_request(
            "bad_request",
            "`tail_bytes` must be provided at most once",
        )),
    }
}

fn parse_raw_log_tail_bytes(value: &str) -> Result<u64, RestError> {
    let parsed = value.parse::<u64>().map_err(|_| {
        RestError::bad_request(
            "bad_request",
            format!("`tail_bytes` must be an integer between 1 and {MAX_RAW_LOG_TAIL_BYTES}"),
        )
    })?;
    if parsed == 0 || parsed > MAX_RAW_LOG_TAIL_BYTES {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`tail_bytes` must be between 1 and {MAX_RAW_LOG_TAIL_BYTES}"),
        ));
    }
    Ok(parsed)
}

fn query_values<'a>(query: Option<&'a str>, key: &'static str) -> Vec<&'a str> {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter_map(|part| {
            let (part_key, value) = part.split_once('=')?;
            (part_key == key).then_some(value)
        })
        .collect()
}

fn train_plan_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message)
        | KernelError::DatasetStoreUnavailable(message)
        | KernelError::TrainStoreUnavailable(message) => {
            if message.contains("blocked") {
                RestError::conflict("plan_blocked", message)
            } else {
                RestError::store_lookup("train_plan_failed", message)
            }
        }
        other => RestError::kernel("train_plan_failed", other),
    }
}

fn train_run_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message)
        | KernelError::DatasetStoreUnavailable(message)
        | KernelError::TrainStoreUnavailable(message) => {
            RestError::store_lookup("train_run_failed", message)
        }
        KernelError::TrainRuntimeUnavailable(message) => {
            if message.contains("already running") {
                RestError::conflict("run_already_running", message)
            } else {
                RestError::kernel(
                    "train_run_failed",
                    KernelError::TrainRuntimeUnavailable(message),
                )
            }
        }
        other => RestError::kernel("train_run_failed", other),
    }
}

pub async fn start_lora_run_job(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let plan_selector = TrainRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request(
            "bad_request",
            format!("invalid train plan reference: {err}"),
        )
    })?;
    let job = state.app().jobs().create(
        JobKind::lora_train_run(),
        format!("run LoRA plan {reference}"),
        Some(JobTarget::new("train").with_reference(reference)),
        ["train".to_string(), "adapters".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "starting LoRA train run",
        move |registry, job_id| {
            run_lora_train_job(task_state, layout, plan_selector, registry, job_id)
        },
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

fn run_lora_train_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    plan_selector: TrainRefSelector,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let run_usecase = state.app().services().kernel().train_run_usecase();
    let started = run_usecase
        .start_run(LoraTrainRunStartRequest {
            layout: layout.clone(),
            plan_selector,
        })
        .map_err(|error| error.to_string())?;
    let run_ref = started.outcome.run.run_ref.clone();
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("launching LoRA train worker".to_string()),
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("created train run {}", started.outcome.run.short_ref),
            )],
            ..JobProgressUpdate::default()
        },
    );

    let launch = state
        .app()
        .services()
        .kernel()
        .training()
        .worker_launcher()
        .launch_worker(&started.layout.home_dir, &run_ref);
    let pid = match launch {
        Ok(pid) => pid,
        Err(error) => {
            let _ = state
                .app()
                .services()
                .kernel()
                .train_run_usecase()
                .mark_run_failed(LoraTrainRunMarkFailedRequest {
                    layout,
                    run_selector: TrainRefSelector::parse(&run_ref)
                        .map_err(|err| err.to_string())?,
                    phase: "worker_spawn".to_string(),
                    message: error.to_string(),
                    exit_code: None,
                });
            return Err(error.to_string());
        }
    };

    let run = state
        .app()
        .services()
        .kernel()
        .train_run_usecase()
        .record_worker_started(LoraTrainRunWorkerStartedRequest {
            layout: layout.clone(),
            run_selector: TrainRefSelector::parse(&run_ref).map_err(|err| err.to_string())?,
            pid,
        })
        .map_err(|error| error.to_string())?;
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("LoRA train worker running".to_string()),
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("worker pid {pid} for train run {}", run.short_ref),
            )],
            ..JobProgressUpdate::default()
        },
    );

    wait_for_train_run_terminal(state, layout, run.run_ref, registry, job_id)
}

fn wait_for_train_run_terminal(
    state: RestState,
    layout: RuntimeLayoutInput,
    run_ref: String,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let run_selector = TrainRefSelector::parse(&run_ref).map_err(|err| err.to_string())?;
    let mut last_stage = String::new();
    loop {
        std::thread::sleep(TRAIN_RUN_POLL_INTERVAL);
        let inspection = state
            .app()
            .services()
            .kernel()
            .train_run_usecase()
            .inspect_run(LoraTrainRunInspectRequest {
                layout: layout.clone(),
                run_selector: run_selector.clone(),
            })
            .map_err(|error| error.to_string())?;
        let run = inspection.inspection.run;
        let phase = run.phase.clone().unwrap_or_else(|| run.status.to_string());
        let stage = format!("LoRA train {}: {phase}", run.status);
        let output = (stage != last_stage)
            .then(|| JobOutputLine::new(JobStream::Event, stage.clone()))
            .into_iter()
            .collect();
        last_stage = stage.clone();
        registry.update_progress(
            &job_id,
            JobProgressUpdate {
                stage: Some(stage),
                output,
                ..JobProgressUpdate::default()
            },
        );

        match run.status {
            LoraTrainRunStatus::Starting | LoraTrainRunStatus::Running => {}
            LoraTrainRunStatus::Succeeded => {
                let artifact = run
                    .adapter_ref
                    .clone()
                    .map(|adapter_ref| {
                        let mut artifact = JobArtifact::new("adapter").with_reference(adapter_ref);
                        if let Some(path) =
                            run.adapter_store_path.clone().or(run.adapter_path.clone())
                        {
                            artifact = artifact.with_path(path);
                        }
                        artifact
                    })
                    .unwrap_or_else(|| {
                        JobArtifact::new("lora_train_run")
                            .with_reference(run.run_ref.clone())
                            .with_path(run.run_dir.clone())
                    });
                return Ok(JobCompletion::new(format!(
                    "LoRA train run {} succeeded",
                    run.short_ref
                ))
                .with_artifact(artifact));
            }
            LoraTrainRunStatus::Failed => {
                return Err(run
                    .error
                    .unwrap_or_else(|| format!("LoRA train run {} failed", run.short_ref)));
            }
        }
    }
}
