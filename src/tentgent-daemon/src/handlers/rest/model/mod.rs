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
        domain::{HfModelPullProgress, ModelCapability, ModelRefSelector},
        usecases::{
            ModelCapabilityMutation, ModelCapabilityProofListRequest, ModelCapabilityProofUseCase,
            ModelCapabilityUpdateRequest, ModelCapabilityUpdateUseCase,
            ModelCapabilityVerifyRequest, ModelCatalogReadUseCase, ModelHfPullRequest,
            ModelHfPullUseCase, ModelInspectRequest, ModelListRequest, ModelLocalImportRequest,
            ModelLocalImportUseCase, ModelRemoveRequest, ModelRemoveUseCase,
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
    model_capability_proofs_response, model_capability_update_response,
    model_capability_verify_response, model_inspection_item, model_mutation_response,
    model_removal_item, model_summary_item, ModelCapabilityProofsResponse,
    ModelCapabilityUpdateResponse, ModelCapabilityVerifyResponse, ModelMutationResponse,
    ModelResponse, ModelsResponse,
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

pub async fn update_capability(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(request): Json<ModelCapabilityUpdateRequestBody>,
) -> Result<Json<ModelCapabilityUpdateResponse>, RestError> {
    let capability = parse_required_model_capability("capability", &request.capability)?;
    update_capabilities_with_mutation(
        state,
        reference,
        ModelCapabilityMutation::Set(vec![capability]),
    )
    .await
}

pub async fn update_capabilities(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(request): Json<ModelCapabilitiesUpdateRequestBody>,
) -> Result<Json<ModelCapabilityUpdateResponse>, RestError> {
    let mutation = parse_capability_mutation_request(request)?;
    update_capabilities_with_mutation(state, reference, mutation).await
}

pub async fn capability_proofs(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<ModelCapabilityProofsResponse>, RestError> {
    let selector = ModelRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid model reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .capability_proof_usecase()
        .list_model_capability_proofs(ModelCapabilityProofListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(model_error)?;

    Ok(Json(model_capability_proofs_response(
        result.model,
        result.proofs,
    )))
}

pub async fn verify_capability(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(request): Json<ModelCapabilityVerifyRequestBody>,
) -> Result<Json<ModelCapabilityVerifyResponse>, RestError> {
    let selector = ModelRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid model reference: {err}"))
    })?;
    let capability = parse_required_model_capability("capability", &request.capability)?;
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .capability_proof_usecase()
        .verify_model_capability(ModelCapabilityVerifyRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            selector,
            capability,
        })
        .map_err(model_error)?;

    Ok(Json(model_capability_verify_response(
        result.model,
        result.proof,
    )))
}

async fn update_capabilities_with_mutation(
    state: RestState,
    reference: String,
    mutation: ModelCapabilityMutation,
) -> Result<Json<ModelCapabilityUpdateResponse>, RestError> {
    let selector = ModelRefSelector::parse(&reference).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid model reference: {err}"))
    })?;
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .capability_update_usecase()
        .update_model_capability(ModelCapabilityUpdateRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            selector,
            mutation,
        })
        .map_err(model_capability_error)?;

    Ok(Json(model_capability_update_response(result)))
}

pub async fn import(
    State(state): State<RestState>,
    Json(request): Json<ModelImportJobRequest>,
) -> Result<Json<ModelMutationResponse>, RestError> {
    let source_path = canonical_import_path(&request.path)?;
    let capability = parse_model_capability(request.capability.as_deref())?;
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .local_import_usecase()
        .import_local_model(ModelLocalImportRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            source_path,
            capability,
        })
        .map_err(model_mutation_error)?;

    Ok(Json(model_mutation_response(result.outcome, "import")))
}

pub async fn pull(
    State(state): State<RestState>,
    Json(request): Json<ModelPullJobRequest>,
) -> Result<Json<ModelMutationResponse>, RestError> {
    let repo_id = normalize_repo_id(&request.repo_id)?;
    let revision = normalize_revision(request.revision)?;
    let capability = parse_model_capability(request.capability.as_deref())?;
    let task_state = state.clone();
    let layout = state.app().layout_input(LayoutResolveMode::Create);
    let result = tokio::task::spawn_blocking(move || {
        task_state
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
                    capability,
                    auth: AuthSecretResolutionRequest::for_secret_use(
                        Provider::HuggingFace,
                        AuthEnvLoadPolicy::CwdDotenvOverride,
                    ),
                },
                &mut |_| {},
            )
    })
    .await
    .map_err(|err| RestError::internal("pull_failed", format!("model pull task failed: {err}")))?
    .map_err(model_mutation_error)?;

    Ok(Json(model_mutation_response(result.outcome, "pull")))
}

pub async fn import_job(
    State(state): State<RestState>,
    Json(request): Json<ModelImportJobRequest>,
) -> Result<(StatusCode, Json<JobResponse>), RestError> {
    let source_path = canonical_import_path(&request.path)?;
    let capability = parse_model_capability(request.capability.as_deref())?;
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
            run_model_import_job(task_state, layout, source_path, capability)
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
    let capability = parse_model_capability(request.capability.as_deref())?;
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
            run_model_pull_job(
                task_state, layout, repo_id, revision, capability, registry, job_id,
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
pub struct ModelImportJobRequest {
    pub path: String,
    pub capability: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPullJobRequest {
    pub repo_id: String,
    pub revision: Option<String>,
    pub capability: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCapabilityUpdateRequestBody {
    pub capability: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCapabilitiesUpdateRequestBody {
    pub set: Option<Vec<String>>,
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCapabilityVerifyRequestBody {
    pub capability: String,
}

fn run_model_import_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    source_path: std::path::PathBuf,
    capability: Option<ModelCapability>,
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
            capability,
        })
        .map_err(|error| error.to_string())?;

    Ok(job_completion_for_import_outcome(
        "imported",
        result.outcome,
    ))
}

fn run_model_pull_job(
    state: RestState,
    layout: RuntimeLayoutInput,
    repo_id: String,
    revision: Option<String>,
    capability: Option<ModelCapability>,
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
                capability,
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

    Ok(job_completion_for_import_outcome("pulled", result.outcome))
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

fn parse_model_capability(value: Option<&str>) -> Result<Option<ModelCapability>, RestError> {
    value
        .map(|value| parse_required_model_capability("capability", value))
        .transpose()
}

fn parse_required_model_capability(field: &str, value: &str) -> Result<ModelCapability, RestError> {
    value
        .parse()
        .map_err(|err| RestError::bad_request("bad_request", format!("invalid {field}: {err}")))
}

fn parse_capability_mutation_request(
    request: ModelCapabilitiesUpdateRequestBody,
) -> Result<ModelCapabilityMutation, RestError> {
    let has_set = request.set.is_some();
    if has_set && (!request.add.is_empty() || !request.remove.is_empty()) {
        return Err(RestError::bad_request(
            "bad_request",
            "use either `set` or `add`/`remove`, not both",
        ));
    }

    if let Some(set) = request.set {
        let set = parse_model_capability_list("set", set)?;
        if set.is_empty() {
            return Err(RestError::bad_request(
                "bad_request",
                "`set` must contain at least one capability",
            ));
        }
        return Ok(ModelCapabilityMutation::Set(set));
    }

    let add = parse_model_capability_list("add", request.add)?;
    let remove = parse_model_capability_list("remove", request.remove)?;
    if add.is_empty() && remove.is_empty() {
        return Err(RestError::bad_request(
            "bad_request",
            "request must include `set`, `add`, or `remove` capabilities",
        ));
    }

    Ok(ModelCapabilityMutation::AddRemove { add, remove })
}

fn parse_model_capability_list(
    field: &str,
    values: Vec<String>,
) -> Result<Vec<ModelCapability>, RestError> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| parse_required_model_capability(&format!("{field}[{index}]"), &value))
        .collect()
}

fn job_completion_for_import_outcome(
    verb: &str,
    outcome: tentgent_kernel::features::model::domain::ModelImportOutcome,
) -> JobCompletion {
    let warning = outcome.metadata.capability_warning().map(str::to_string);
    let metadata = outcome.metadata;
    let mut completion = JobCompletion::new(format!("{verb} model {}", metadata.short_ref))
        .with_artifact(
            JobArtifact::new("model")
                .with_reference(metadata.model_ref.into_string())
                .with_path(outcome.store_path.display().to_string()),
        );
    if let Some(warning) = warning {
        completion = completion.with_warning_summary(warning);
    }
    completion
}

fn model_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("model_read_failed", message)
        }
        other => RestError::kernel("model_read_failed", other),
    }
}

fn model_capability_error(error: KernelError) -> RestError {
    match error {
        KernelError::UnsupportedTarget(message) => RestError::bad_request("bad_request", message),
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("model_capability_update_failed", message)
        }
        other => RestError::kernel("model_capability_update_failed", other),
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

fn model_mutation_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("store_mutation_failed", message)
        }
        KernelError::RuntimeStateUnavailable(message) => {
            RestError::conflict("provider_runtime_failed", message)
        }
        other => RestError::kernel("store_mutation_failed", other),
    }
}
