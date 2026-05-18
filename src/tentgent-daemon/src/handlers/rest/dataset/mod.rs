mod dto;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tentgent_kernel::{
    features::dataset::{
        domain::{DatasetRefSelector, DatasetSynthRequest},
        usecases::{
            DatasetCatalogReadUseCase, DatasetDiffRequest, DatasetDiffRightSelection,
            DatasetDiffUseCase, DatasetEvaluateRequest, DatasetEvaluationInputSelection,
            DatasetEvaluationUseCase, DatasetExportRequest, DatasetExportUseCase,
            DatasetInspectRequest, DatasetListRequest, DatasetLocalImportRequest,
            DatasetLocalImportUseCase, DatasetRemoveRequest, DatasetRemoveUseCase,
            DatasetSynthesisUseCase, DatasetSynthesizeRequest, DatasetTemplateRenderRequest,
            DatasetTemplateUseCase, DatasetValidateRequest, DatasetValidationTargetSelection,
            DatasetValidationUseCase,
        },
    },
    features::runtime::domain::PythonRuntimeResolutionInput,
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::{
    handlers::rest::{
        jobs::{job_item, JobResponse},
        store_jobs::{
            canonical_import_path, dataset_auth_request, ensure_missing_or_empty_dir,
            optional_content, optional_f32, optional_max_u32, optional_positive_u32,
            optional_range_f32, parse_eval_input, parse_eval_split, parse_synth_source,
            required_absolute_path, required_dataset_provider, required_string, synth_counts,
            ParsedEvalInput,
        },
    },
    runtime::{
        JobArtifact, JobCompletion, JobId, JobKind, JobOutputLine, JobProgressPatch,
        JobProgressUpdate, JobRegistry, JobStream, JobTarget,
    },
    transport::rest::{error::RestError, state::RestState},
};

use self::dto::{
    dataset_diff_response, dataset_export_response, dataset_inspection_item,
    dataset_mutation_response, dataset_removal_item, dataset_summary_item,
    dataset_template_response, dataset_validation_response, DatasetDiffResponse,
    DatasetExportResponse, DatasetMutationResponse, DatasetResponse, DatasetTemplateResponse,
    DatasetValidationResponse, DatasetsResponse,
};

pub async fn list(State(state): State<RestState>) -> Result<Json<DatasetsResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .catalog_usecase()
        .list_datasets(DatasetListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(dataset_error)?;

    Ok(Json(DatasetsResponse {
        datasets: result
            .datasets
            .into_iter()
            .map(dataset_summary_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<DatasetResponse>, RestError> {
    let selector = DatasetRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid dataset reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .catalog_usecase()
        .inspect_dataset(DatasetInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(dataset_error)?;

    Ok(Json(DatasetResponse {
        dataset: dataset_inspection_item(result.dataset),
    }))
}

pub async fn remove(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<DatasetResponse>, RestError> {
    let selector = DatasetRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid dataset reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .remove_usecase()
        .remove_dataset(DatasetRemoveRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            selector,
        })
        .map_err(dataset_remove_error)?;

    Ok(Json(DatasetResponse {
        dataset: dataset_removal_item(result.outcome),
    }))
}

pub async fn validate(
    State(state): State<RestState>,
    Json(request): Json<DatasetValidateBody>,
) -> Result<Json<DatasetValidationResponse>, RestError> {
    let target = match (request.path, request.dataset_ref) {
        (Some(path), None) => {
            DatasetValidationTargetSelection::LocalPath(canonical_import_path(&path)?)
        }
        (None, Some(reference)) => {
            DatasetValidationTargetSelection::ManagedDataset(parse_dataset_ref(&reference)?)
        }
        _ => {
            return Err(RestError::bad_request(
                "bad_request",
                "exactly one of `path` or `dataset_ref` is required",
            ))
        }
    };
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .validation_usecase()
        .validate_dataset(DatasetValidateRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            target,
        })
        .map_err(dataset_error)?;

    Ok(Json(dataset_validation_response(
        result.outcome,
        result.dataset,
    )))
}

pub async fn template(
    State(state): State<RestState>,
    Json(request): Json<DatasetTemplateBody>,
) -> Result<Json<DatasetTemplateResponse>, RestError> {
    let template = tentgent_kernel::features::dataset::domain::DatasetTemplateRequest::new(
        request.task,
        request.language,
    );
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .template_usecase()
        .render_dataset_template(DatasetTemplateRenderRequest {
            template: template.clone(),
            output_path: None,
        })
        .map_err(dataset_error)?;

    Ok(Json(dataset_template_response(template, result.rendered)))
}

pub async fn export(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(request): Json<DatasetExportBody>,
) -> Result<Json<DatasetExportResponse>, RestError> {
    let selector = parse_dataset_ref(&reference)?;
    let destination_path = required_absolute_path(Some(&request.output_path), "output_path")?;
    ensure_missing_or_empty_dir(&destination_path)?;
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .export_usecase()
        .export_dataset(DatasetExportRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            selector,
            destination_path,
        })
        .map_err(dataset_mutation_error)?;

    let store_path = result
        .store
        .dataset_dir(&result.outcome.metadata.dataset_ref);
    Ok(Json(dataset_export_response(result.outcome, &store_path)))
}

pub async fn diff(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(request): Json<DatasetDiffBody>,
) -> Result<Json<DatasetDiffResponse>, RestError> {
    let left = parse_dataset_ref(&reference)?;
    let right = match (request.right_dataset_ref, request.right_path) {
        (Some(reference), None) => {
            DatasetDiffRightSelection::ManagedDataset(parse_dataset_ref(&reference)?)
        }
        (None, Some(path)) => DatasetDiffRightSelection::LocalPath(canonical_import_path(&path)?),
        _ => {
            return Err(RestError::bad_request(
                "bad_request",
                "exactly one of `right_dataset_ref` or `right_path` is required",
            ))
        }
    };
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .diff_usecase()
        .diff_dataset(DatasetDiffRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            left,
            right,
        })
        .map_err(dataset_error)?;

    Ok(Json(dataset_diff_response(result.outcome)))
}

pub async fn import(
    State(state): State<RestState>,
    Json(request): Json<DatasetImportJobRequest>,
) -> Result<Json<DatasetMutationResponse>, RestError> {
    let source_path = canonical_import_path(&request.path)?;
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .local_import_usecase()
        .import_local_dataset(DatasetLocalImportRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            source_path,
        })
        .map_err(dataset_mutation_error)?;

    Ok(Json(dataset_mutation_response(result.outcome, "import")))
}

pub async fn import_job(
    State(state): State<RestState>,
    Json(request): Json<DatasetImportJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let source_path = canonical_import_path(&request.path)?;
    let job = state.app().jobs().create(
        JobKind::dataset_import(),
        format!("import {}", source_path.display()),
        Some(JobTarget::new("datasets").with_path(source_path.display().to_string())),
        ["datasets".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);

    state
        .app()
        .job_runner()
        .spawn_blocking(registry, job_id, "importing dataset", move |_, _| {
            run_dataset_import_job(task_state, layout, source_path)
        });

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn synth_job(
    State(state): State<RestState>,
    Json(request): Json<DatasetSynthJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    if request.print_prompt {
        return Err(RestError::bad_request(
            "bad_request",
            "print_prompt is synchronous and is not accepted by the job endpoint",
        ));
    }

    let source = parse_synth_source(
        &state.app().layout().home_dir,
        request.brief,
        request.spec_content,
        request.spec_path,
    )?;
    let (split, counts) = synth_counts(
        request.split,
        request.count,
        request.train_count,
        request.valid_count,
        request.test_count,
        request.eval_count,
    )?;
    let provider = required_dataset_provider(request.provider.as_deref())?;
    let provider_model = required_string(request.model.as_deref(), "model")?;
    let output_dir = required_absolute_path(request.output_path.as_deref(), "output_path")?;
    ensure_missing_or_empty_dir(&output_dir)?;
    let max_tokens = optional_positive_u32(request.max_tokens, "max_tokens")?;
    let temperature = optional_f32(request.temperature, 0.0, "temperature")?;
    let timeout_seconds = optional_range_f32(request.timeout_seconds, 180.0, "timeout_seconds")?;
    let retries = optional_max_u32(request.retries, 1, 10, "retries")?;

    let expected_jobs = counts.expected_jobs();
    let synth = DatasetSynthRequest {
        provider,
        provider_model,
        output_dir: output_dir.clone(),
        prompt_source: source.prompt_source,
        split,
        counts,
        max_tokens,
        temperature,
        timeout_seconds,
        retries,
    };
    let job = state.app().jobs().create(
        JobKind::dataset_synthesis(),
        source.label,
        Some(JobTarget::new("datasets").with_path(output_dir.display().to_string())),
        ["datasets".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let handle = tokio::runtime::Handle::current();

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "running dataset synthesis",
        move |registry, job_id| {
            run_dataset_synth_job(
                task_state,
                layout,
                synth,
                expected_jobs,
                handle,
                registry,
                job_id,
            )
        },
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn eval_job(
    State(state): State<RestState>,
    Json(request): Json<DatasetEvalJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let provider = required_dataset_provider(request.provider.as_deref())?;
    let provider_model = required_string(request.model.as_deref(), "model")?;
    let output_dir = required_absolute_path(request.output_path.as_deref(), "output_path")?;
    ensure_missing_or_empty_dir(&output_dir)?;
    let (input, label) = parse_eval_input(
        &state.app().layout().home_dir,
        request.dataset_ref,
        request.input_content,
        request.input_format,
        request.input_path,
    )?;
    let split = parse_eval_split(request.split)?;
    let max_records = optional_positive_u32(request.max_records, "max_records")?.unwrap_or(20);
    let criteria = optional_content(request.criteria, "criteria")?;
    let max_tokens = optional_positive_u32(request.max_tokens, "max_tokens")?;
    let temperature = optional_f32(request.temperature, 0.0, "temperature")?;
    let timeout_seconds = optional_range_f32(request.timeout_seconds, 180.0, "timeout_seconds")?;

    let input = match input {
        ParsedEvalInput::LocalPath(path) => DatasetEvaluationInputSelection::LocalPath(path),
        ParsedEvalInput::ManagedDataset(selector) => {
            DatasetEvaluationInputSelection::ManagedDataset(selector)
        }
    };
    let job = state.app().jobs().create(
        JobKind::dataset_evaluation(),
        label,
        Some(JobTarget::new("datasets").with_path(output_dir.display().to_string())),
        ["datasets".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let handle = tokio::runtime::Handle::current();

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "running dataset evaluation",
        move |registry, job_id| {
            run_dataset_eval_job(
                task_state,
                layout,
                provider,
                provider_model,
                input,
                output_dir,
                split,
                max_records,
                criteria,
                max_tokens,
                temperature,
                timeout_seconds,
                handle,
                registry,
                job_id,
            )
        },
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetImportJobRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetValidateBody {
    pub path: Option<String>,
    pub dataset_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetTemplateBody {
    pub task: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetExportBody {
    pub output_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetDiffBody {
    pub right_dataset_ref: Option<String>,
    pub right_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetSynthJobRequest {
    #[serde(default)]
    pub print_prompt: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub output_path: Option<String>,
    pub brief: Option<String>,
    pub spec_content: Option<String>,
    pub spec_path: Option<String>,
    pub split: Option<String>,
    pub count: Option<u32>,
    pub train_count: Option<u32>,
    pub valid_count: Option<u32>,
    pub test_count: Option<u32>,
    pub eval_count: Option<u32>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub timeout_seconds: Option<f32>,
    pub retries: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetEvalJobRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub output_path: Option<String>,
    pub dataset_ref: Option<String>,
    pub input_content: Option<String>,
    pub input_format: Option<String>,
    pub input_path: Option<String>,
    pub split: Option<String>,
    pub max_records: Option<u32>,
    pub criteria: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub timeout_seconds: Option<f32>,
}

fn run_dataset_import_job(
    state: RestState,
    layout: tentgent_kernel::foundation::layout::RuntimeLayoutInput,
    source_path: std::path::PathBuf,
) -> Result<JobCompletion, String> {
    let result = state
        .app()
        .services()
        .kernel()
        .datasets()
        .local_import_usecase()
        .import_local_dataset(DatasetLocalImportRequest {
            layout,
            source_path,
        })
        .map_err(|error| error.to_string())?;

    let metadata = result.outcome.metadata;
    Ok(
        JobCompletion::new(format!("imported dataset {}", metadata.short_ref)).with_artifact(
            JobArtifact::new("dataset")
                .with_reference(metadata.dataset_ref.into_string())
                .with_path(result.outcome.store_path.display().to_string()),
        ),
    )
}

fn run_dataset_synth_job(
    state: RestState,
    layout: tentgent_kernel::foundation::layout::RuntimeLayoutInput,
    synth: DatasetSynthRequest,
    expected_jobs: u64,
    handle: tokio::runtime::Handle,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("running dataset synthesis runtime".to_string()),
            progress: JobProgressPatch {
                files_done: Some(0),
                files_total: Some(expected_jobs),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                "dataset synthesis runtime started",
            )],
            warning_summary: None,
        },
    );
    let output_dir = synth.output_dir.clone();
    let provider = synth.provider;
    let result = handle
        .block_on(async {
            state
                .app()
                .services()
                .kernel()
                .dataset_synthesis_usecase()
                .synthesize_dataset(DatasetSynthesizeRequest {
                    layout,
                    runtime: PythonRuntimeResolutionInput::default(),
                    auth: dataset_auth_request(provider),
                    synth,
                })
                .await
        })
        .map_err(|error| error.to_string())?;
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("dataset synthesis completed".to_string()),
            progress: JobProgressPatch {
                files_done: Some(expected_jobs),
                files_total: Some(expected_jobs),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!(
                    "captured {} progress event(s)",
                    result.output.progress_events.len()
                ),
            )],
            warning_summary: result
                .output
                .progress_truncated
                .then(|| "dataset synthesis progress output was truncated".to_string()),
        },
    );

    Ok(JobCompletion::new(format!(
        "dataset synthesis completed with {} progress event(s)",
        result.output.progress_events.len()
    ))
    .with_artifact(
        JobArtifact::new("dataset_synthesis").with_path(output_dir.display().to_string()),
    ))
}

#[allow(clippy::too_many_arguments)]
fn run_dataset_eval_job(
    state: RestState,
    layout: tentgent_kernel::foundation::layout::RuntimeLayoutInput,
    provider: tentgent_kernel::features::dataset::domain::DatasetProvider,
    provider_model: String,
    input: DatasetEvaluationInputSelection,
    output_dir: std::path::PathBuf,
    split: tentgent_kernel::features::dataset::domain::DatasetEvalSplit,
    max_records: u32,
    criteria: Option<String>,
    max_tokens: Option<u32>,
    temperature: f32,
    timeout_seconds: f32,
    handle: tokio::runtime::Handle,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("running dataset evaluation runtime".to_string()),
            progress: JobProgressPatch {
                files_done: Some(0),
                files_total: Some(1),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                "dataset evaluation runtime started",
            )],
            warning_summary: None,
        },
    );
    let result = handle
        .block_on(async {
            state
                .app()
                .services()
                .kernel()
                .dataset_evaluation_usecase()
                .evaluate_dataset(DatasetEvaluateRequest {
                    layout,
                    runtime: PythonRuntimeResolutionInput::default(),
                    auth: dataset_auth_request(provider),
                    provider,
                    provider_model,
                    input,
                    output_dir: output_dir.clone(),
                    split,
                    max_records,
                    criteria,
                    max_tokens,
                    temperature,
                    timeout_seconds,
                })
                .await
        })
        .map_err(|error| error.to_string())?;
    registry.update_progress(
        &job_id,
        JobProgressUpdate {
            stage: Some("dataset evaluation completed".to_string()),
            progress: JobProgressPatch {
                files_done: Some(1),
                files_total: Some(1),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                format!("evaluated input {}", result.input_path.display()),
            )],
            warning_summary: None,
        },
    );

    Ok(
        JobCompletion::new("dataset evaluation completed").with_artifact(
            JobArtifact::new("dataset_evaluation").with_path(output_dir.display().to_string()),
        ),
    )
}

fn parse_dataset_ref(reference: &str) -> Result<DatasetRefSelector, RestError> {
    DatasetRefSelector::parse(reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid dataset reference: {err}"))
    })
}

fn dataset_error(error: KernelError) -> RestError {
    match error {
        KernelError::DatasetStoreUnavailable(message) => {
            RestError::store_lookup("dataset_read_failed", message)
        }
        other => RestError::kernel("dataset_read_failed", other),
    }
}

fn dataset_mutation_error(error: KernelError) -> RestError {
    match error {
        KernelError::DatasetStoreUnavailable(message) if message.contains("already exists") => {
            RestError::conflict("output_exists", message)
        }
        KernelError::DatasetStoreUnavailable(message) => {
            RestError::store_lookup("dataset_mutation_failed", message)
        }
        other => RestError::kernel("dataset_mutation_failed", other),
    }
}

fn dataset_remove_error(error: KernelError) -> RestError {
    match error {
        KernelError::DatasetStoreUnavailable(message) if message.contains("still referenced") => {
            RestError::conflict("dataset_in_use", message)
        }
        KernelError::DatasetStoreUnavailable(message) => {
            RestError::store_lookup("dataset_remove_failed", message)
        }
        other => RestError::kernel("dataset_remove_failed", other),
    }
}
