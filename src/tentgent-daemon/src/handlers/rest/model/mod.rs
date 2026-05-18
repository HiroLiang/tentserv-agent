mod dto;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tentgent_kernel::{
    features::auth::{
        domain::{AuthEnvLoadPolicy, Provider},
        usecases::AuthSecretResolutionRequest,
    },
    features::model::{
        domain::{HfModelPullProgress, ModelRefSelector},
        usecases::{
            ModelCatalogReadUseCase, ModelHfPullRequest, ModelHfPullUseCase, ModelInspectRequest,
            ModelListRequest, ModelLocalImportRequest, ModelLocalImportUseCase, ModelRemoveRequest,
            ModelRemoveUseCase,
        },
    },
    features::runtime::domain::PythonRuntimeResolutionInput,
    foundation::{
        error::KernelError,
        layout::{LayoutResolveMode, RuntimeLayoutInput},
    },
};

use crate::{
    handlers::rest::{
        jobs::{job_item, JobResponse},
        store_jobs::{
            canonical_import_path, hf_progress_update, normalize_repo_id, normalize_revision,
        },
    },
    runtime::{
        JobArtifact, JobCompletion, JobId, JobKind, JobProgressUpdate, JobRegistry, JobTarget,
    },
    transport::rest::{error::RestError, state::RestState},
};

use self::dto::{
    model_inspection_item, model_removal_item, model_summary_item, ModelResponse, ModelsResponse,
};

pub async fn list(State(state): State<RestState>) -> Result<Json<ModelsResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .catalog_usecase()
        .list_models(ModelListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(model_error)?;

    Ok(Json(ModelsResponse {
        models: result.models.into_iter().map(model_summary_item).collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<ModelResponse>, RestError> {
    let selector = ModelRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid model reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .catalog_usecase()
        .inspect_model(ModelInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(model_error)?;

    Ok(Json(ModelResponse {
        model: model_inspection_item(result.model),
    }))
}

pub async fn remove(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<ModelResponse>, RestError> {
    let selector = ModelRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid model reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .remove_usecase()
        .remove_model(ModelRemoveRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            selector,
        })
        .map_err(model_remove_error)?;

    Ok(Json(ModelResponse {
        model: model_removal_item(result.outcome),
    }))
}

pub async fn import_job(
    State(state): State<RestState>,
    Json(request): Json<ModelImportJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let source_path = canonical_import_path(&request.path)?;
    let job = state.app().jobs().create(
        JobKind::model_import(),
        format!("import {}", source_path.display()),
        Some(JobTarget::new("models").with_path(source_path.display().to_string())),
        ["models".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);

    state
        .app()
        .job_runner()
        .spawn_blocking(registry, job_id, "importing model", move |_, _| {
            run_model_import_job(task_state, layout, source_path)
        });

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn pull_job(
    State(state): State<RestState>,
    Json(request): Json<ModelPullJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let repo_id = normalize_repo_id(&request.repo_id)?;
    let revision = normalize_revision(request.revision)?;
    let label = revision
        .as_deref()
        .map(|revision| format!("{repo_id}@{revision}"))
        .unwrap_or_else(|| repo_id.clone());
    let job = state.app().jobs().create(
        JobKind::model_pull(),
        label,
        Some(JobTarget::new("models").with_reference(repo_id.clone())),
        ["models".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "starting Hugging Face model pull",
        move |registry, job_id| {
            run_model_pull_job(task_state, layout, repo_id, revision, registry, job_id)
        },
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelImportJobRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPullJobRequest {
    pub repo_id: String,
    pub revision: Option<String>,
}

fn run_model_import_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    source_path: std::path::PathBuf,
) -> Result<JobCompletion, String> {
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .local_import_usecase()
        .import_local_model(ModelLocalImportRequest {
            layout,
            source_path,
        })
        .map_err(|error| error.to_string())?;

    let metadata = result.outcome.metadata;
    Ok(
        JobCompletion::new(format!("imported model {}", metadata.short_ref)).with_artifact(
            JobArtifact::new("model")
                .with_reference(metadata.model_ref.into_string())
                .with_path(result.outcome.store_path.display().to_string()),
        ),
    )
}

fn run_model_pull_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    repo_id: String,
    revision: Option<String>,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let result = state
        .app()
        .services()
        .kernel()
        .model_hf_pull_usecase()
        .pull_hf_model(
            ModelHfPullRequest {
                layout,
                runtime: PythonRuntimeResolutionInput::default(),
                repo_id,
                revision,
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::CwdDotenvOverride,
                ),
            },
            &mut |event| {
                registry.update_progress(&job_id, model_progress_update(event));
            },
        )
        .map_err(|error| error.to_string())?;

    let metadata = result.outcome.metadata;
    Ok(
        JobCompletion::new(format!("pulled model {}", metadata.short_ref)).with_artifact(
            JobArtifact::new("model")
                .with_reference(metadata.model_ref.into_string())
                .with_path(result.outcome.store_path.display().to_string()),
        ),
    )
}

fn model_progress_update(progress: HfModelPullProgress) -> JobProgressUpdate {
    hf_progress_update(
        progress.description,
        progress.position,
        progress.total,
        &progress.unit,
        progress.finished,
    )
}

fn model_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("model_read_failed", message)
        }
        other => RestError::kernel("model_read_failed", other),
    }
}

fn model_remove_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) if message.contains("still referenced") => {
            RestError::conflict("model_in_use", message)
        }
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("model_remove_failed", message)
        }
        other => RestError::kernel("model_remove_failed", other),
    }
}
