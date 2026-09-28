use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use tentgent_kernel::{
    features::{
        model::{
            domain::{ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelRefSelector},
            infra::{FileModelCapabilityProofStore, FileModelCatalogStore, SystemModelClock},
            usecases::{
                ModelCapabilityProofRecordRequest, ModelCapabilityProofUseCase,
                StdModelCapabilityProofUseCase,
            },
        },
        runtime::infra::{
            preload_model_runtime, ModelRuntimeDaemonEndpoint, ModelRuntimePreloadFailureKind,
        },
        server::options::LoadMode,
    },
    foundation::{
        error::KernelResult,
        layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver},
    },
};

use super::{
    capability::ensure_model_endpoint, error::LocalServerError, ingress::reject_lifecycle_path,
    LocalServerState,
};

#[derive(Clone)]
pub(in crate::server) struct StartupReadiness(Arc<AtomicBool>);

impl StartupReadiness {
    pub(in crate::server) fn new(mode: LoadMode) -> Self {
        Self(Arc::new(AtomicBool::new(mode == LoadMode::Lazy)))
    }

    pub(super) fn is_ready(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(super) fn mark_ready(&self) {
        self.0.store(true, Ordering::Release);
    }
}

pub(super) async fn admit_ready_request(
    State(state): State<LocalServerState>,
    request: Request,
    next: Next,
) -> Result<Response, LocalServerError> {
    reject_lifecycle_path(request.uri().path())?;
    if request.uri().path() != "/healthz" && !state.readiness.is_ready() {
        return Err(LocalServerError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "server_starting",
            message: "local server is still validating eager model loading; retry after health reports ready".into(),
        });
    }
    Ok(next.run(request).await)
}

pub(super) async fn prepare_local_startup(
    state: &LocalServerState,
) -> Result<(), LocalServerError> {
    prepare_with_endpoint(state, ensure_model_endpoint(state)).await
}

pub(super) async fn prepare_with_endpoint(
    state: &LocalServerState,
    endpoint: impl Future<Output = Result<ModelRuntimeDaemonEndpoint, LocalServerError>>,
) -> Result<(), LocalServerError> {
    if state.config.load_mode == LoadMode::Lazy {
        return Ok(());
    }
    // This runs on BOTH newly spawned and reused generations. Managed Python
    // launch stays lazy; its first-spawner idle/ownership policy is unchanged.
    let endpoint = endpoint.await?;
    let result = preload_model_runtime(&endpoint).await;
    let evidence = match &result {
        Ok(_) => Some((ModelCapabilityProofStatus::Verified, None)),
        Err(error) if error.kind == ModelRuntimePreloadFailureKind::LoadFailed => {
            Some((ModelCapabilityProofStatus::Failed, Some(error.to_string())))
        }
        _ => None, // Observation/transport failures are not model incompatibility.
    };
    if let Some((status, error)) = evidence {
        if let Err(error) = record_startup_evidence(state, status, error) {
            tracing::warn!(%error, "could not persist local eager startup evidence");
        }
    }
    // Never terminate or release all resources of a shared Python generation
    // when this local server cannot complete its own startup.
    result
        .map(|_| ())
        .map_err(|error| LocalServerError::internal(error.to_string()))
}

fn record_startup_evidence(
    state: &LocalServerState,
    status: ModelCapabilityProofStatus,
    error: Option<String>,
) -> KernelResult<()> {
    let profile = state
        .config
        .runtime_profile
        .as_deref()
        .map(tentgent_kernel::features::server::domain::ServerRuntimeProfileSelection::parse_label)
        .transpose()
        .map_err(|error| {
            tentgent_kernel::foundation::error::KernelError::ServerStoreUnavailable(error)
        })?;
    StdModelCapabilityProofUseCase::new(
        &StdRuntimeLayoutResolver,
        &FileModelCatalogStore,
        &FileModelCapabilityProofStore,
        &SystemModelClock,
    )
    .record_model_capability_proof(ModelCapabilityProofRecordRequest {
        layout: RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(state.layout.home_dir.clone()),
            data_root_dir: Some(state.layout.data_root_dir.clone()),
        },
        selector: ModelRefSelector::parse(&state.config.model_ref).map_err(|error| {
            tentgent_kernel::foundation::error::KernelError::ModelStoreUnavailable(
                error.to_string(),
            )
        })?,
        capability: state.config.capability.required_model_capability(),
        status,
        source: ModelCapabilityProofSource::ServerStart,
        server_ref: Some(state.config.server_ref.clone()),
        runtime_profile: profile.as_ref().map(|profile| profile.profile_id.clone()),
        runtime_profile_version: profile.map(|profile| profile.profile_version),
        error,
    })?;
    Ok(())
}
