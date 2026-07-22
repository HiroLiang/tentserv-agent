use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tentgent_kernel::{
    features::adapter::{
        domain::{
            AdapterBackendSupport, AdapterFormat, AdapterRefSelector, AdapterType,
            HfAdapterPullProgress, LoraScale,
        },
        usecases::{
            AdapterBindRequest, AdapterCatalogReadUseCase, AdapterHfPullRequest,
            AdapterHfPullUseCase, AdapterImportOptions, AdapterInspectRequest, AdapterListRequest,
            AdapterLocalImportRequest, AdapterLocalImportUseCase, AdapterRemoveRequest,
        },
    },
    features::auth::{
        domain::{AuthEnvLoadPolicy, Provider},
        usecases::AuthSecretResolutionRequest,
    },
    features::model::domain::{ModelCapability, ModelRefSelector},
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

use super::dto::{
    adapter_bind_response, adapter_import_mutation_response, adapter_inspection_item,
    adapter_removal_item, adapter_summary_item, AdapterMutationResponse, AdapterResponse,
    AdaptersResponse,
};

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

pub async fn remove(
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
        .remove_usecase()
        .remove_adapter_guarded(AdapterRemoveRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            selector,
        })
        .map_err(adapter_remove_error)?;
    let result = RestError::guarded(result)?;

    Ok(Json(AdapterResponse {
        adapter: adapter_removal_item(result.outcome),
    }))
}

pub async fn import(
    State(state): State<RestState>,
    Json(request): Json<AdapterImportJobRequest>,
) -> Result<Json<AdapterMutationResponse>, RestError> {
    let options = request.import_options()?;
    let source_path = canonical_import_path(&request.path)?;
    let base_model_selector =
        normalize_optional_model_ref(request.base_model_ref, "base_model_ref")?;
    let result = state
        .app()
        .services()
        .kernel()
        .adapters()
        .local_import_usecase(state.app().services().kernel().models().catalog_store())
        .import_local_adapter(AdapterLocalImportRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            source_path,
            base_model_selector,
            options,
        })
        .map_err(adapter_mutation_error)?;

    Ok(Json(adapter_import_mutation_response(
        result.outcome,
        "import",
    )))
}

pub async fn pull(
    State(state): State<RestState>,
    Json(request): Json<AdapterPullJobRequest>,
) -> Result<Json<AdapterMutationResponse>, RestError> {
    let options = request.import_options()?;
    let repo_id = normalize_repo_id(&request.repo_id)?;
    let revision = normalize_revision(request.revision)?;
    let base_model_selector =
        normalize_optional_model_ref(request.base_model_ref, "base_model_ref")?;
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let result = tokio::task::spawn_blocking(move || {
        task_state
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
                    options,
                    auth: AuthSecretResolutionRequest::for_secret_use(
                        Provider::HuggingFace,
                        AuthEnvLoadPolicy::CwdDotenvOverride,
                    ),
                },
                &mut |_| {},
            )
    })
    .await
    .map_err(|err| RestError::internal("pull_failed", format!("adapter pull task failed: {err}")))?
    .map_err(adapter_mutation_error)?;

    Ok(Json(adapter_import_mutation_response(
        result.outcome,
        "pull",
    )))
}

pub async fn bind(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(request): Json<AdapterBindBody>,
) -> Result<Json<AdapterMutationResponse>, RestError> {
    let adapter_selector = AdapterRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid adapter reference: {err}"))
    })?;
    let base_model_selector =
        normalize_optional_model_ref(Some(request.base_model_ref), "base_model_ref")?.ok_or_else(
            || RestError::bad_request("bad_request", "base_model_ref must not be blank"),
        )?;
    let result = state
        .app()
        .services()
        .kernel()
        .adapters()
        .bind_usecase(state.app().services().kernel().models().catalog_store())
        .bind_adapter_guarded(AdapterBindRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            adapter_selector,
            base_model_selector,
        })
        .map_err(adapter_mutation_error)?;
    let result = RestError::guarded(result)?;

    Ok(Json(adapter_bind_response(result.outcome)))
}

pub async fn import_job(
    State(state): State<RestState>,
    Json(request): Json<AdapterImportJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let options = request.import_options()?;
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
            run_adapter_import_job(
                task_state,
                layout,
                source_path,
                base_model_selector,
                options,
            )
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
    let options = request.import_options()?;
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
            run_adapter_pull_job(AdapterPullJobInput {
                state: task_state,
                layout,
                repo_id,
                revision,
                base_model_selector,
                options,
                registry,
                job_id,
            })
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
    pub target_capability: Option<String>,
    pub adapter_type: Option<String>,
    pub adapter_format: Option<String>,
    #[serde(default)]
    pub backend_support: Vec<String>,
    pub control_kind: Option<String>,
    pub weight_file: Option<String>,
    #[serde(default)]
    pub trigger_words: Vec<String>,
    pub recommended_scale: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterPullJobRequest {
    pub repo_id: String,
    pub revision: Option<String>,
    pub base_model_ref: Option<String>,
    pub target_capability: Option<String>,
    pub adapter_type: Option<String>,
    pub adapter_format: Option<String>,
    #[serde(default)]
    pub backend_support: Vec<String>,
    pub control_kind: Option<String>,
    pub weight_file: Option<String>,
    #[serde(default)]
    pub trigger_words: Vec<String>,
    pub recommended_scale: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterBindBody {
    pub base_model_ref: String,
}

fn run_adapter_import_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    source_path: std::path::PathBuf,
    base_model_selector: Option<ModelRefSelector>,
    options: AdapterImportOptions,
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
            options,
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

struct AdapterPullJobInput {
    state: RestState,
    layout: RuntimeLayoutInput,
    repo_id: String,
    revision: Option<String>,
    base_model_selector: Option<ModelRefSelector>,
    options: AdapterImportOptions,
    registry: JobRegistry,
    job_id: JobId,
}

fn run_adapter_pull_job(input: AdapterPullJobInput) -> Result<JobCompletion, String> {
    let result = input
        .state
        .app()
        .services()
        .kernel()
        .adapter_hf_pull_usecase()
        .pull_hf_adapter(
            AdapterHfPullRequest {
                layout: input.layout,
                runtime: PythonRuntimeResolutionInput::default(),
                repo_id: input.repo_id,
                revision: input.revision,
                base_model_selector: input.base_model_selector,
                options: input.options,
                auth: AuthSecretResolutionRequest::for_secret_use(
                    Provider::HuggingFace,
                    AuthEnvLoadPolicy::CwdDotenvOverride,
                ),
            },
            &mut |event| {
                input
                    .registry
                    .update_progress(&input.job_id, adapter_progress_update(event));
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

impl AdapterImportJobRequest {
    fn import_options(&self) -> Result<AdapterImportOptions, RestError> {
        adapter_import_options(AdapterImportOptionFields::from_import(self))
    }
}

impl AdapterPullJobRequest {
    fn import_options(&self) -> Result<AdapterImportOptions, RestError> {
        adapter_import_options(AdapterImportOptionFields::from_pull(self))
    }
}

struct AdapterImportOptionFields<'a> {
    target_capability: Option<&'a str>,
    adapter_type: Option<&'a str>,
    adapter_format: Option<&'a str>,
    backend_support: &'a [String],
    control_kind: Option<&'a str>,
    weight_file: Option<&'a str>,
    trigger_words: &'a [String],
    recommended_scale: Option<f32>,
}

impl<'a> AdapterImportOptionFields<'a> {
    fn from_import(request: &'a AdapterImportJobRequest) -> Self {
        Self {
            target_capability: request.target_capability.as_deref(),
            adapter_type: request.adapter_type.as_deref(),
            adapter_format: request.adapter_format.as_deref(),
            backend_support: &request.backend_support,
            control_kind: request.control_kind.as_deref(),
            weight_file: request.weight_file.as_deref(),
            trigger_words: &request.trigger_words,
            recommended_scale: request.recommended_scale,
        }
    }

    fn from_pull(request: &'a AdapterPullJobRequest) -> Self {
        Self {
            target_capability: request.target_capability.as_deref(),
            adapter_type: request.adapter_type.as_deref(),
            adapter_format: request.adapter_format.as_deref(),
            backend_support: &request.backend_support,
            control_kind: request.control_kind.as_deref(),
            weight_file: request.weight_file.as_deref(),
            trigger_words: &request.trigger_words,
            recommended_scale: request.recommended_scale,
        }
    }
}

fn adapter_import_options(
    fields: AdapterImportOptionFields<'_>,
) -> Result<AdapterImportOptions, RestError> {
    let target_capability = fields
        .target_capability
        .map(|value| {
            value.parse::<ModelCapability>().map_err(|err| {
                RestError::bad_request("bad_request", format!("invalid target_capability: {err}"))
            })
        })
        .transpose()?;
    let adapter_type = fields
        .adapter_type
        .map(|value| {
            value.parse::<AdapterType>().map_err(|err| {
                RestError::bad_request("bad_request", format!("invalid adapter_type: {err}"))
            })
        })
        .transpose()?;
    let adapter_format = fields
        .adapter_format
        .map(|value| {
            value.parse::<AdapterFormat>().map_err(|err| {
                RestError::bad_request("bad_request", format!("invalid adapter_format: {err}"))
            })
        })
        .transpose()?;
    let backend_support = fields
        .backend_support
        .iter()
        .map(|value| {
            value.parse::<AdapterBackendSupport>().map_err(|err| {
                RestError::bad_request("bad_request", format!("invalid backend_support: {err}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let trigger_words = fields
        .trigger_words
        .iter()
        .filter_map(|value| non_empty_string(value))
        .collect::<Vec<_>>();
    let recommended_scale = fields
        .recommended_scale
        .map(|value| {
            LoraScale::new(value)
                .map_err(|err| RestError::bad_request("bad_request", err.to_string()))
        })
        .transpose()?;

    Ok(AdapterImportOptions {
        adapter_type,
        target_capability,
        adapter_format,
        backend_support,
        control_kind: fields.control_kind.and_then(non_empty_string),
        weight_file: fields.weight_file.and_then(non_empty_string),
        trigger_words,
        recommended_scale,
    })
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
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

fn adapter_remove_error(error: KernelError) -> RestError {
    match error {
        KernelError::AdapterStoreUnavailable(message) if message.contains("still referenced") => {
            RestError::conflict("adapter_in_use", message)
        }
        KernelError::AdapterStoreUnavailable(message) => {
            RestError::store_lookup("adapter_remove_failed", message)
        }
        other => RestError::kernel("adapter_remove_failed", other),
    }
}

fn adapter_mutation_error(error: KernelError) -> RestError {
    match error {
        KernelError::AdapterStoreUnavailable(message) => {
            RestError::store_lookup("adapter_mutation_failed", message)
        }
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("model_read_failed", message)
        }
        KernelError::RuntimeStateUnavailable(message) => {
            RestError::conflict("provider_runtime_failed", message)
        }
        other => RestError::kernel("adapter_mutation_failed", other),
    }
}
