mod dto;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tentgent_kernel::{
    features::adapter::{
        domain::{AdapterRefSelector, HfAdapterPullProgress},
        usecases::{
            AdapterCatalogReadUseCase, AdapterHfPullRequest, AdapterHfPullUseCase,
            AdapterInspectRequest, AdapterListRequest, AdapterLocalImportRequest,
            AdapterLocalImportUseCase,
        },
    },
    features::auth::{
        domain::{AuthEnvLoadPolicy, Provider},
        usecases::AuthSecretResolutionRequest,
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
            canonical_import_path, hf_progress_update, normalize_optional_model_ref,
            normalize_repo_id, normalize_revision,
        },
    },
    runtime::{
        JobArtifact, JobCompletion, JobId, JobKind, JobProgressUpdate, JobRegistry, JobTarget,
    },
    transport::rest::{error::RestError, state::RestState},
};

use self::dto::{adapter_inspection_item, adapter_summary_item, AdapterResponse, AdaptersResponse};

pub async fn list(State(state): State<RestState>) -> Result<Json<AdaptersResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .adapters()
        .catalog_usecase()
        .list_adapters(AdapterListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(adapter_error)?;

    Ok(Json(AdaptersResponse {
        adapters: result
            .adapters
            .into_iter()
            .map(adapter_summary_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<AdapterResponse>, RestError> {
    let selector = AdapterRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid adapter reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .adapters()
        .catalog_usecase()
        .inspect_adapter(AdapterInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(adapter_error)?;

    Ok(Json(AdapterResponse {
        adapter: adapter_inspection_item(result.adapter),
    }))
}

pub async fn import_job(
    State(state): State<RestState>,
    Json(request): Json<AdapterImportJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let source_path = canonical_import_path(&request.path)?;
    let base_model_selector =
        normalize_optional_model_ref(request.base_model_ref, "base_model_ref")?;
    let job = state.app().jobs().create(
        JobKind::adapter_import(),
        format!("import {}", source_path.display()),
        Some(JobTarget::new("adapters").with_path(source_path.display().to_string())),
        ["adapters".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);

    state
        .app()
        .job_runner()
        .spawn_blocking(registry, job_id, "importing adapter", move |_, _| {
            run_adapter_import_job(task_state, layout, source_path, base_model_selector)
        });

    Ok((
        StatusCode::ACCEPTED,
        Json(JobResponse { job: job_item(job) }),
    ))
}

pub async fn pull_job(
    State(state): State<RestState>,
    Json(request): Json<AdapterPullJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let repo_id = normalize_repo_id(&request.repo_id)?;
    let revision = normalize_revision(request.revision)?;
    let base_model_selector =
        normalize_optional_model_ref(request.base_model_ref, "base_model_ref")?;
    let label = revision
        .as_deref()
        .map(|revision| format!("{repo_id}@{revision}"))
        .unwrap_or_else(|| repo_id.clone());
    let job = state.app().jobs().create(
        JobKind::adapter_pull(),
        label,
        Some(JobTarget::new("adapters").with_reference(repo_id.clone())),
        ["adapters".to_string()],
    );
    let job_id = job.job_id.clone();
    let registry = state.app().jobs().clone();
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);

    state.app().job_runner().spawn_blocking(
        registry,
        job_id,
        "starting Hugging Face adapter pull",
        move |registry, job_id| {
            run_adapter_pull_job(
                task_state,
                layout,
                repo_id,
                revision,
                base_model_selector,
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
pub struct AdapterImportJobRequest {
    pub path: String,
    pub base_model_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterPullJobRequest {
    pub repo_id: String,
    pub revision: Option<String>,
    pub base_model_ref: Option<String>,
}

fn run_adapter_import_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    source_path: std::path::PathBuf,
    base_model_selector: Option<tentgent_kernel::features::model::domain::ModelRefSelector>,
) -> Result<JobCompletion, String> {
    let result = state
        .app()
        .services()
        .kernel()
        .adapters()
        .local_import_usecase(state.app().services().kernel().models().catalog_store())
        .import_local_adapter(AdapterLocalImportRequest {
            layout,
            source_path,
            base_model_selector,
        })
        .map_err(|error| error.to_string())?;

    let metadata = result.outcome.metadata;
    Ok(
        JobCompletion::new(format!("imported adapter {}", metadata.short_ref)).with_artifact(
            JobArtifact::new("adapter")
                .with_reference(metadata.adapter_ref.into_string())
                .with_path(result.outcome.store_path.display().to_string()),
        ),
    )
}

fn run_adapter_pull_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    repo_id: String,
    revision: Option<String>,
    base_model_selector: Option<tentgent_kernel::features::model::domain::ModelRefSelector>,
    registry: JobRegistry,
    job_id: JobId,
) -> Result<JobCompletion, String> {
    let result = state
        .app()
        .services()
        .kernel()
        .adapter_hf_pull_usecase()
        .pull_hf_adapter(
            AdapterHfPullRequest {
                layout,
                runtime: PythonRuntimeResolutionInput::default(),
                repo_id,
                revision,
                base_model_selector,
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::CwdDotenvOverride,
                ),
            },
            &mut |event| {
                registry.update_progress(&job_id, adapter_progress_update(event));
            },
        )
        .map_err(|error| error.to_string())?;

    let metadata = result.outcome.metadata;
    Ok(
        JobCompletion::new(format!("pulled adapter {}", metadata.short_ref)).with_artifact(
            JobArtifact::new("adapter")
                .with_reference(metadata.adapter_ref.into_string())
                .with_path(result.outcome.store_path.display().to_string()),
        ),
    )
}

fn adapter_progress_update(progress: HfAdapterPullProgress) -> JobProgressUpdate {
    hf_progress_update(
        progress.description,
        progress.position,
        progress.total,
        &progress.unit,
        progress.finished,
    )
}

fn adapter_error(error: KernelError) -> RestError {
    match error {
        KernelError::AdapterStoreUnavailable(message) => {
            RestError::store_lookup("adapter_read_failed", message)
        }
        other => RestError::kernel("adapter_read_failed", other),
    }
}
