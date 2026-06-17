use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tentgent_kernel::{
    features::{
        auth::{
            domain::{AuthEnvLoadPolicy, AuthSecretMaterial, AuthValidationState, Provider},
            infra::ReqwestAuthSecretValidator,
            ports::AuthSecretValidator,
            usecases::{AuthSecretResolutionRequest, AuthSecretResolverUseCase},
        },
        model::{
            domain::{ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelRefSelector},
            usecases::{ModelCapabilityProofRecordRequest, ModelCapabilityProofUseCase},
        },
        runtime::{
            domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput},
            usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase},
        },
        server::{
            domain::{
                CloudProvider, LaunchMode, ServerCapability, ServerInspection, ServerRuntimeKind,
            },
            infra::{ServerRuntimeLaunchRequest, ServerRuntimeLauncher},
            usecases::{
                ServerInspectRequest, ServerLifecycleUseCase, ServerListRequest,
                ServerPrepareRequest, ServerRecordProcessStartRequest, ServerRemoveRequest,
                ServerResolveForStartRequest, ServerSpecUseCase, ServerStopRequest,
            },
        },
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayout},
};

use crate::transport::rest::{error::RestError, state::RestState};

use super::{
    common::{layout_input_from_layout, parse_server_selector},
    dto::{
        server_inspection_item, server_remove_response, server_stop_response, server_summary_item,
        ServerCreateResponse, ServerResponse, ServerStartResponse, ServersResponse,
    },
    error::{auth_error, server_error},
    health::wait_for_server_ready,
};

const DEFAULT_START_TIMEOUT_SECONDS: u64 = 30;
const MAX_START_TIMEOUT_SECONDS: u64 = 120;

pub async fn list(State(state): State<RestState>) -> Result<Json<ServersResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .list_servers(ServerListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            running_only: false,
        })
        .map_err(server_error)?;

    Ok(Json(ServersResponse {
        servers: result
            .servers
            .into_iter()
            .map(server_summary_item)
            .collect(),
    }))
}

pub async fn create(
    State(state): State<RestState>,
    Json(request): Json<ServerCreateRequest>,
) -> Result<(StatusCode, Json<ServerCreateResponse>), RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .prepare_server(ServerPrepareRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            runtime_ref: request.runtime_ref,
            capability: request.capability,
            host: request.host,
            port: request.port,
            lazy_load: request.lazy_load.unwrap_or(false),
            idle_seconds: request.idle_seconds,
            allow_unverified: request.allow_unverified.unwrap_or(false),
        })
        .map_err(server_error)?;

    let status = if result.outcome.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(ServerCreateResponse {
            server: server_inspection_item(result.outcome.inspection),
            created: result.outcome.created,
        }),
    ))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<ServerResponse>, RestError> {
    let selector = parse_server_selector(&reference)?;
    let result = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .inspect_server(ServerInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(server_error)?;

    Ok(Json(ServerResponse {
        server: server_inspection_item(result.inspection),
    }))
}

pub async fn remove(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<super::dto::ServerRemoveResponse>, RestError> {
    let selector = parse_server_selector(&reference)?;
    let result = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .remove_server(ServerRemoveRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            selector,
        })
        .map_err(server_error)?;

    Ok(Json(server_remove_response(result.outcome)))
}

pub async fn start(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    body: Bytes,
) -> Result<Json<ServerStartResponse>, RestError> {
    let request = parse_start_request(&body)?;
    let wait_ready = request.wait_ready.unwrap_or(false);
    let timeout_seconds = validate_start_timeout(request.timeout_seconds)?;
    let selector = parse_server_selector(&reference)?;
    let plan = {
        let result = state
            .app()
            .services()
            .kernel()
            .server_usecase()
            .resolve_for_start(ServerResolveForStartRequest {
                layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
                selector,
                allow_unverified: request.allow_unverified.unwrap_or(false),
            })
            .map_err(server_error)?;
        let auth = resolve_server_runtime_auth(&state, &result.inspection)?;
        let runtime = state
            .app()
            .services()
            .kernel()
            .runtime()
            .resolve_runtime(RuntimeResolutionRequest {
                layout: layout_input_from_layout(&result.layout, LayoutResolveMode::ReadOnly),
                runtime: PythonRuntimeResolutionInput::default(),
            })
            .map_err(server_error)?
            .runtime;
        ServerStartPlan {
            layout: result.layout,
            inspection: result.inspection,
            runtime,
            auth,
        }
    };
    let ServerStartPlan {
        layout,
        inspection,
        runtime,
        auth,
    } = plan;
    let auth = validate_server_runtime_auth(auth).await?;
    let recorded_inspection = {
        let spawned = {
            let launcher = ServerRuntimeLauncher::new(state.app().services().kernel().runtime());
            match launcher.spawn_background(ServerRuntimeLaunchRequest {
                layout: layout.clone(),
                runtime,
                inspection: inspection.clone(),
                auth,
            }) {
                Ok(spawned) => spawned,
                Err(err) => {
                    let message = err.to_string();
                    let _ = record_local_server_capability_proof(
                        &state,
                        &layout,
                        &inspection,
                        ModelCapabilityProofStatus::Failed,
                        Some(message),
                    );
                    return Err(server_error(err));
                }
            }
        };
        match state
            .app()
            .services()
            .kernel()
            .server_usecase()
            .record_process_start(ServerRecordProcessStartRequest {
                layout: layout_input_from_layout(&layout, LayoutResolveMode::ReadOnly),
                server_ref: inspection.spec.server_ref.clone(),
                pid: spawned.pid,
                bound_port: spawned.bound_port,
                launch_mode: LaunchMode::Background,
            }) {
            Ok(result) => result.inspection,
            Err(err) => {
                let message = err.to_string();
                let _ = record_local_server_capability_proof(
                    &state,
                    &layout,
                    &inspection,
                    ModelCapabilityProofStatus::Failed,
                    Some(message),
                );
                return Err(server_error(err));
            }
        }
    };

    let readiness = if wait_ready {
        let readiness = wait_for_server_ready(&recorded_inspection, timeout_seconds).await;
        let (status, error) = if readiness.ready {
            (ModelCapabilityProofStatus::Verified, None)
        } else {
            (
                ModelCapabilityProofStatus::Failed,
                readiness
                    .error
                    .clone()
                    .or_else(|| Some("server readiness check did not pass".to_string())),
            )
        };
        let _ = record_local_server_capability_proof(
            &state,
            &layout,
            &recorded_inspection,
            status,
            error,
        );
        Some(readiness)
    } else {
        let _ = record_local_server_capability_proof(
            &state,
            &layout,
            &recorded_inspection,
            ModelCapabilityProofStatus::Verified,
            None,
        );
        None
    };
    drop(state);

    Ok(Json(ServerStartResponse {
        server: server_inspection_item(recorded_inspection),
        readiness,
    }))
}

fn record_local_server_capability_proof(
    state: &RestState,
    layout: &RuntimeLayout,
    inspection: &ServerInspection,
    status: ModelCapabilityProofStatus,
    error: Option<String>,
) -> Result<(), tentgent_kernel::foundation::error::KernelError> {
    let Some(model_ref) = inspection.spec.local_model_ref() else {
        return Ok(());
    };
    let selector = ModelRefSelector::parse(model_ref.as_str()).map_err(|err| {
        tentgent_kernel::foundation::error::KernelError::ModelStoreUnavailable(format!(
            "invalid model ref in server spec: {err}"
        ))
    })?;
    state
        .app()
        .services()
        .kernel()
        .models()
        .capability_proof_usecase()
        .record_model_capability_proof(ModelCapabilityProofRecordRequest {
            layout: layout_input_from_layout(layout, LayoutResolveMode::Create),
            selector,
            capability: inspection.spec.capability.required_model_capability(),
            status,
            source: ModelCapabilityProofSource::ServerStart,
            server_ref: Some(inspection.spec.server_ref.to_string()),
            runtime_profile: inspection
                .spec
                .runtime_profile
                .as_ref()
                .map(|profile| profile.profile_id.clone()),
            runtime_profile_version: inspection
                .spec
                .runtime_profile
                .as_ref()
                .map(|profile| profile.profile_version),
            error,
        })?;
    Ok(())
}

pub async fn stop(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<super::dto::ServerStopResponse>, RestError> {
    let selector = parse_server_selector(&reference)?;
    let result = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .stop_server(ServerStopRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(server_error)?;

    Ok(Json(server_stop_response(result.outcome)))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerCreateRequest {
    pub runtime_ref: String,
    pub capability: Option<ServerCapability>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub lazy_load: Option<bool>,
    pub idle_seconds: Option<u64>,
    pub allow_unverified: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerStartRequest {
    pub wait_ready: Option<bool>,
    pub timeout_seconds: Option<u64>,
    pub allow_unverified: Option<bool>,
}

struct ServerStartPlan {
    layout: RuntimeLayout,
    inspection: ServerInspection,
    runtime: PythonRuntimeLayout,
    auth: Option<AuthSecretMaterial>,
}

fn validate_start_timeout(timeout_seconds: Option<u64>) -> Result<u64, RestError> {
    let timeout_seconds = timeout_seconds.unwrap_or(DEFAULT_START_TIMEOUT_SECONDS);
    if timeout_seconds == 0 || timeout_seconds > MAX_START_TIMEOUT_SECONDS {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`timeout_seconds` must be between 1 and {MAX_START_TIMEOUT_SECONDS}"),
        ));
    }
    Ok(timeout_seconds)
}

fn parse_start_request(body: &[u8]) -> Result<ServerStartRequest, RestError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(ServerStartRequest::default());
    }
    serde_json::from_slice(body).map_err(|err| {
        RestError::bad_request(
            "bad_request",
            format!("invalid server start request: {err}"),
        )
    })
}

fn resolve_server_runtime_auth(
    state: &RestState,
    inspection: &ServerInspection,
) -> Result<Option<AuthSecretMaterial>, RestError> {
    if inspection.spec.runtime_kind == ServerRuntimeKind::Local {
        return Ok(None);
    }

    let provider = cloud_auth_provider(inspection.spec.provider, &inspection.spec.short_ref)?;
    let resolution = state
        .app()
        .services()
        .kernel()
        .auth()
        .resolver_usecase()
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            provider,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        ))
        .map_err(auth_error)?;

    resolution.secret.map(Some).ok_or_else(|| {
        RestError::conflict(
            "provider_auth_missing",
            format!(
                "{} key is missing for cloud server `{}`; run `tentgent auth {} set` or set `{}` before launch",
                provider.display_name(),
                inspection.spec.short_ref,
                provider.cli_name(),
                provider.env_var()
            ),
        )
    })
}

async fn validate_server_runtime_auth(
    auth: Option<AuthSecretMaterial>,
) -> Result<Option<AuthSecretMaterial>, RestError> {
    let Some(auth) = auth else {
        return Ok(None);
    };
    let provider = auth.provider;
    let validator = ReqwestAuthSecretValidator::new().map_err(auth_error)?;
    match validator
        .validate(provider, auth.secret())
        .await
        .map_err(auth_error)?
    {
        AuthValidationState::Verified => Ok(Some(auth)),
        AuthValidationState::Missing => Err(RestError::conflict(
            "provider_auth_missing",
            format!(
                "{} key is missing for cloud server launch",
                provider.display_name()
            ),
        )),
        AuthValidationState::Invalid { reason } => Err(RestError::conflict(
            "provider_auth_invalid",
            format!(
                "{} key is invalid for cloud server launch: {reason}",
                provider.display_name()
            ),
        )),
        AuthValidationState::Unknown { reason } => Err(RestError::conflict(
            "provider_auth_unknown",
            format!(
                "{} key could not be verified for cloud server launch: {reason}",
                provider.display_name()
            ),
        )),
        AuthValidationState::NotChecked => Err(RestError::conflict(
            "provider_auth_unknown",
            format!(
                "{} key validation was not checked for cloud server launch",
                provider.display_name()
            ),
        )),
    }
}

fn cloud_auth_provider(
    provider: Option<CloudProvider>,
    short_ref: &str,
) -> Result<Provider, RestError> {
    match provider {
        Some(CloudProvider::OpenAI) => Ok(Provider::OpenAI),
        Some(CloudProvider::Anthropic) => Ok(Provider::Anthropic),
        Some(CloudProvider::Gemini) => Ok(Provider::Gemini),
        None => Err(RestError::conflict(
            "provider_auth_missing",
            format!("cloud server `{short_ref}` is missing provider metadata"),
        )),
    }
}
