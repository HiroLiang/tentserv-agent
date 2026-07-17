//! Standard server spec and lifecycle orchestration.

use std::sync::Arc;

use crate::features::cluster::ports::ClusterCatalogStore;
use crate::features::model::domain::ModelStoreLayout;
use crate::features::model::ports::{ModelCapabilityProofStore, ModelCatalogStore};
use crate::features::resource_coordination::{
    infra::FileResourceCoordinator, ResourceCoordinator, ResourceKey, ResourceKind,
    ResourceLockMode, ResourceLockRequest,
};
use crate::features::resource_guard::{
    ResourceGuardUseCase, ResourceMutationAuthorization, ResourceMutationOutcome,
    ResourceOperation, StdResourceGuard,
};
use crate::features::runtime::infra::ModelRuntimeCapability;
use crate::features::runtime_ownership::{RuntimeExecutionIdentity, RuntimeOwnershipScope};
use crate::features::server::domain::ServerProcessIdentityStatus;
use crate::features::server::domain::{
    ServerCapability, ServerPrepareOutcome, ServerRuntimeKind, ServerSpec, ServerStopOutcome,
};
use crate::features::server::infra::StdServerProcessIdentityProbe;
use crate::features::server::ports::{
    ServerCatalogStore, ServerClock, ServerIdentityGenerator, ServerProcessController,
    ServerProcessIdentityProbe, ServerStoreLayoutInitializer,
};
use crate::features::server::profile::local_server_runtime_profile_for;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    build_server_spec, ensure_server_spec_launchable, resolve_server_runtime_target,
    server_runtime_backend_for_format, server_store_layout,
};
use super::port::{
    ServerClearProcessRequest, ServerInspectRequest, ServerInspectResult, ServerLifecycleUseCase,
    ServerListRequest, ServerListResult, ServerPrepareRequest, ServerPrepareResult,
    ServerRecordProcessStartRequest, ServerRemoveRequest, ServerRemoveResult,
    ServerResolveForStartRequest, ServerSpecUseCase, ServerStartAuthorization, ServerStopRequest,
    ServerStopResult,
};

/// Standard orchestration for server specs and process metadata.
pub struct StdServerUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn ServerStoreLayoutInitializer,
    model_catalog: &'a dyn ModelCatalogStore,
    model_proofs: &'a dyn ModelCapabilityProofStore,
    cluster_catalog: &'a dyn ClusterCatalogStore,
    identity: &'a dyn ServerIdentityGenerator,
    catalog: &'a dyn ServerCatalogStore,
    process_controller: &'a dyn ServerProcessController,
    clock: &'a dyn ServerClock,
    coordinator: Arc<dyn ResourceCoordinator>,
    guard: Arc<dyn ResourceGuardUseCase>,
    process_identity: Arc<dyn ServerProcessIdentityProbe>,
}

impl<'a> StdServerUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn ServerStoreLayoutInitializer,
        model_catalog: &'a dyn ModelCatalogStore,
        model_proofs: &'a dyn ModelCapabilityProofStore,
        identity: &'a dyn ServerIdentityGenerator,
        catalog: &'a dyn ServerCatalogStore,
        process_controller: &'a dyn ServerProcessController,
        clock: &'a dyn ServerClock,
    ) -> Self {
        static CLUSTER_CATALOG: crate::features::cluster::infra::FileClusterCatalogStore =
            crate::features::cluster::infra::FileClusterCatalogStore;
        Self::new_with_cluster_catalog(
            layout_resolver,
            layout_initializer,
            model_catalog,
            model_proofs,
            &CLUSTER_CATALOG,
            identity,
            catalog,
            process_controller,
            clock,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_cluster_catalog(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn ServerStoreLayoutInitializer,
        model_catalog: &'a dyn ModelCatalogStore,
        model_proofs: &'a dyn ModelCapabilityProofStore,
        cluster_catalog: &'a dyn ClusterCatalogStore,
        identity: &'a dyn ServerIdentityGenerator,
        catalog: &'a dyn ServerCatalogStore,
        process_controller: &'a dyn ServerProcessController,
        clock: &'a dyn ServerClock,
    ) -> Self {
        Self::new_with_dependencies(
            layout_resolver,
            layout_initializer,
            model_catalog,
            model_proofs,
            cluster_catalog,
            identity,
            catalog,
            process_controller,
            clock,
            Arc::new(FileResourceCoordinator),
            Arc::new(StdResourceGuard::default()),
            Arc::new(StdServerProcessIdentityProbe),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dependencies(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn ServerStoreLayoutInitializer,
        model_catalog: &'a dyn ModelCatalogStore,
        model_proofs: &'a dyn ModelCapabilityProofStore,
        cluster_catalog: &'a dyn ClusterCatalogStore,
        identity: &'a dyn ServerIdentityGenerator,
        catalog: &'a dyn ServerCatalogStore,
        process_controller: &'a dyn ServerProcessController,
        clock: &'a dyn ServerClock,
        coordinator: Arc<dyn ResourceCoordinator>,
        guard: Arc<dyn ResourceGuardUseCase>,
        process_identity: Arc<dyn ServerProcessIdentityProbe>,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            model_catalog,
            model_proofs,
            cluster_catalog,
            identity,
            catalog,
            process_controller,
            clock,
            coordinator,
            guard,
            process_identity,
        }
    }

    pub fn remove_server_guarded(
        &self,
        request: ServerRemoveRequest,
    ) -> KernelResult<ResourceMutationOutcome<ServerRemoveResult>> {
        let inspected = self.inspect_server(ServerInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let server_ref = inspected.inspection.spec.server_ref.to_string();
        let authorization = self.guard.authorize(
            &inspected.layout,
            ResourceOperation::DeleteServerSpec {
                server_ref: server_ref.clone(),
            },
        )?;
        let _permit = match authorization {
            ResourceMutationAuthorization::Permitted(permit) => permit,
            ResourceMutationAuthorization::Rejected(rejection) => {
                return Ok(ResourceMutationOutcome::Blocked(rejection));
            }
            ResourceMutationAuthorization::Busy(busy) => {
                return Ok(ResourceMutationOutcome::Busy(busy));
            }
        };
        let inspection = self.catalog.inspect_server(
            &inspected.store,
            &crate::features::server::domain::ServerRefSelector::parse(&server_ref)
                .map_err(|error| KernelError::ServerStoreUnavailable(error.to_string()))?,
        )?;
        if inspection.running {
            return Ok(ResourceMutationOutcome::Blocked(
                crate::features::resource_guard::ResourceGuardRejection {
                    code: crate::features::resource_guard::ResourceGuardCode::ServerInUse,
                    operation: "delete-server-spec".to_string(),
                    resource: server_ref.clone(),
                    description: "server spec is still running".to_string(),
                    blockers: vec![crate::features::resource_guard::ResourceBlocker {
                        kind: "server-process".to_string(),
                        code: crate::features::resource_guard::ResourceBlockerCode::ServerRunning,
                        reference: server_ref.clone(),
                        reason: "server process is still running".to_string(),
                        resource_kind: Some("server".to_string()),
                        resource_ref: Some(server_ref.clone()),
                        operation: Some("delete-server-spec".to_string()),
                        capability: None,
                        route: None,
                        field: Some("pid".to_string()),
                        owner: None,
                        next_actions: vec!["stop the server before removing it".to_string()],
                    }],
                },
            ));
        }
        let outcome = self
            .catalog
            .remove_server(&inspected.store, &inspection.spec.server_ref)?;
        Ok(ResourceMutationOutcome::Applied(ServerRemoveResult {
            layout: inspected.layout,
            store: inspected.store,
            outcome,
        }))
    }

    pub fn runtime_ownership_scope_for_server(
        &self,
        layout: &crate::foundation::layout::RuntimeLayout,
        spec: &ServerSpec,
    ) -> KernelResult<RuntimeOwnershipScope> {
        let runtime_identities = if spec.runtime_kind == ServerRuntimeKind::Local {
            let model_ref = spec.model_ref.as_ref().ok_or_else(|| {
                KernelError::ServerStoreUnavailable(
                    "local server spec is missing model_ref".to_string(),
                )
            })?;
            let capability = spec.capability.unwrap_or(ServerCapability::Chat);
            let effective_profile = match spec.runtime_profile.as_ref() {
                Some(profile) => Some(profile.clone()),
                None => {
                    let metadata = self.model_catalog.load_model_metadata(
                        &ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
                        model_ref,
                    )?;
                    let backend =
                        server_runtime_backend_for_format(capability, metadata.primary_format)?;
                    local_server_runtime_profile_for(capability, backend)
                        .map(|profile| profile.selection)
                }
            };
            vec![RuntimeExecutionIdentity::model_bound(
                model_ref.to_string(),
                ModelRuntimeCapability::from_model_capability(
                    capability.required_model_capability(),
                ),
                effective_profile.as_ref(),
            )]
        } else {
            Vec::new()
        };
        Ok(RuntimeOwnershipScope::Server {
            server_ref: spec.server_ref.to_string(),
            runtime_identities,
        })
    }

    pub fn resolve_for_start_guarded(
        &self,
        request: ServerResolveForStartRequest,
    ) -> KernelResult<ServerStartAuthorization> {
        let preflight = self.inspect_server(ServerInspectRequest {
            layout: request.layout.clone(),
            selector: request.selector.clone(),
        })?;
        let permit = match self.coordinator.acquire(
            &preflight.layout,
            ResourceLockRequest::new(
                "start-server",
                server_transition_locks(&preflight.inspection.spec),
            ),
        )? {
            Ok(permit) => permit,
            Err(busy) => {
                return Err(KernelError::ResourceCoordinationUnavailable(format!(
                    "{}; retry after {} ms",
                    busy.description, busy.retry_after_millis
                )));
            }
        };
        let result = self.inspect_server(ServerInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        ensure_server_spec_launchable(
            &result.inspection.spec,
            &result.layout,
            self.model_catalog,
            self.model_proofs,
            self.cluster_catalog,
            request.allow_unverified,
        )?;
        if result.inspection.running {
            return Err(KernelError::ServerRuntimeUnavailable(format!(
                "server `{}` is already running",
                result.inspection.spec.short_ref
            )));
        }
        Ok(ServerStartAuthorization { result, permit })
    }
}

impl ServerSpecUseCase for StdServerUseCase<'_> {
    fn prepare_server(&self, request: ServerPrepareRequest) -> KernelResult<ServerPrepareResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let target = resolve_server_runtime_target(
            &request.target,
            &layout,
            self.model_catalog,
            self.model_proofs,
            self.cluster_catalog,
            request.allow_unverified,
        )?;
        let spec = build_server_spec(
            target.clone(),
            request.host.as_deref(),
            request.port,
            request.lazy_load,
            request.idle_seconds,
            self.clock.now_rfc3339()?,
            self.identity,
        )?;
        let mut locks = vec![
            (ResourceKey::maintenance(), ResourceLockMode::Shared),
            (
                ResourceKey::new(ResourceKind::Server, spec.server_ref.to_string()),
                ResourceLockMode::Exclusive,
            ),
        ];
        match &target {
            crate::features::server::domain::ServerRuntimeTarget::LocalModel {
                model_ref,
                capability,
                ..
            } => {
                locks.push((
                    ResourceKey::new(ResourceKind::Model, model_ref.to_string()),
                    ResourceLockMode::Shared,
                ));
                locks.push((
                    ResourceKey::new(
                        ResourceKind::ModelCapability,
                        format!("{model_ref}|{}", capability.required_model_capability()),
                    ),
                    ResourceLockMode::Shared,
                ));
            }
            crate::features::server::domain::ServerRuntimeTarget::Cluster { cluster_ref } => {
                locks.push((
                    ResourceKey::new(ResourceKind::Cluster, cluster_ref.to_string()),
                    ResourceLockMode::Shared,
                ));
            }
            crate::features::server::domain::ServerRuntimeTarget::CloudProvider { .. } => {}
        }
        let _permit = match self.coordinator.acquire(
            &layout,
            ResourceLockRequest::new("prepare-server-spec", locks),
        )? {
            Ok(permit) => permit,
            Err(busy) => {
                return Err(KernelError::ResourceCoordinationUnavailable(format!(
                    "{}; retry after {} ms",
                    busy.description, busy.retry_after_millis
                )))
            }
        };
        resolve_server_runtime_target(
            &request.target,
            &layout,
            self.model_catalog,
            self.model_proofs,
            self.cluster_catalog,
            request.allow_unverified,
        )?;
        let selector =
            crate::features::server::domain::ServerRefSelector::parse(spec.server_ref.as_str())
                .map_err(|err| KernelError::ServerStoreUnavailable(err.to_string()))?;

        if store.server_spec_path(spec.server_ref.as_str()).exists() {
            let inspection = self.catalog.inspect_server(&store, &selector)?;
            ensure_server_spec_launchable(
                &inspection.spec,
                &layout,
                self.model_catalog,
                self.model_proofs,
                self.cluster_catalog,
                request.allow_unverified,
            )?;
            return Ok(ServerPrepareResult {
                layout,
                store,
                outcome: ServerPrepareOutcome {
                    inspection,
                    created: false,
                },
            });
        }

        self.layout_initializer.ensure_server_store_layout(&store)?;
        self.catalog.save_server_spec(&store, &spec)?;
        let inspection = self.catalog.inspect_server(&store, &selector)?;

        Ok(ServerPrepareResult {
            layout,
            store,
            outcome: ServerPrepareOutcome {
                inspection,
                created: true,
            },
        })
    }

    fn list_servers(&self, request: ServerListRequest) -> KernelResult<ServerListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let servers = if request.running_only {
            self.catalog.list_running_servers(&store)?
        } else {
            self.catalog.list_servers(&store)?
        };

        Ok(ServerListResult {
            layout,
            store,
            servers,
        })
    }

    fn inspect_server(&self, request: ServerInspectRequest) -> KernelResult<ServerInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let inspection = self.catalog.inspect_server(&store, &request.selector)?;

        Ok(ServerInspectResult {
            layout,
            store,
            inspection,
        })
    }

    fn remove_server(&self, request: ServerRemoveRequest) -> KernelResult<ServerRemoveResult> {
        self.remove_server_guarded(request)?.into_compat_result()
    }
}

impl ServerLifecycleUseCase for StdServerUseCase<'_> {
    fn resolve_for_start(
        &self,
        request: ServerResolveForStartRequest,
    ) -> KernelResult<ServerInspectResult> {
        Ok(self.resolve_for_start_guarded(request)?.result)
    }

    fn record_process_start(
        &self,
        request: ServerRecordProcessStartRequest,
    ) -> KernelResult<ServerInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let inspection = self.catalog.record_process_start(
            &store,
            &request.server_ref,
            request.pid,
            request.process_token,
            request.bound_port,
            request.launch_mode,
            self.clock.now_rfc3339()?,
        )?;

        Ok(ServerInspectResult {
            layout,
            store,
            inspection,
        })
    }

    fn clear_process_if_matches(&self, request: ServerClearProcessRequest) -> KernelResult<()> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        self.catalog
            .clear_process_if_matches(&store, &request.server_ref, request.expected_pid)
    }

    fn stop_server(&self, request: ServerStopRequest) -> KernelResult<ServerStopResult> {
        let preflight = self.inspect_server(ServerInspectRequest {
            layout: request.layout.clone(),
            selector: request.selector.clone(),
        })?;
        let _permit = match self.coordinator.acquire(
            &preflight.layout,
            ResourceLockRequest::new(
                "stop-server",
                server_transition_locks(&preflight.inspection.spec),
            ),
        )? {
            Ok(permit) => permit,
            Err(busy) => {
                return Err(KernelError::ResourceCoordinationUnavailable(format!(
                    "{}; retry after {} ms",
                    busy.description, busy.retry_after_millis
                )));
            }
        };
        let inspected = self.inspect_server(ServerInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let process = inspected.inspection.process.clone().ok_or_else(|| {
            KernelError::ServerRuntimeUnavailable(format!(
                "server `{}` is not running",
                inspected.inspection.spec.short_ref
            ))
        })?;
        if !inspected.inspection.running {
            return Err(KernelError::ServerRuntimeUnavailable(format!(
                "server `{}` is not running",
                inspected.inspection.spec.short_ref
            )));
        }

        if let Some(token) = process.process_token.as_deref() {
            match self.process_identity.probe_process_identity(
                &inspected.layout,
                inspected.inspection.spec.server_ref.as_str(),
                process.pid,
                token,
            )? {
                ServerProcessIdentityStatus::Matching => {}
                ServerProcessIdentityStatus::Stopped => {
                    return Err(KernelError::ServerRuntimeUnavailable(format!(
                        "server `{}` process identity is no longer active; inspect and reconcile state before retrying",
                        inspected.inspection.spec.short_ref
                    )));
                }
                ServerProcessIdentityStatus::Mismatch { description }
                | ServerProcessIdentityStatus::Unavailable { description } => {
                    return Err(KernelError::ServerRuntimeUnavailable(format!(
                        "server `{}` process identity cannot be verified: {description}; stop the named process manually before reconciling state",
                        inspected.inspection.spec.short_ref
                    )));
                }
            }
        }

        self.process_controller.terminate_process(process.pid)?;
        self.catalog.clear_process_if_matches(
            &inspected.store,
            &inspected.inspection.spec.server_ref,
            Some(process.pid),
        )?;
        let selector = crate::features::server::domain::ServerRefSelector::parse(
            inspected.inspection.spec.server_ref.as_str(),
        )
        .map_err(|err| KernelError::ServerStoreUnavailable(err.to_string()))?;
        let inspection = self.catalog.inspect_server(&inspected.store, &selector)?;

        Ok(ServerStopResult {
            layout: inspected.layout,
            store: inspected.store,
            outcome: ServerStopOutcome {
                inspection,
                stopped_pid: process.pid,
            },
        })
    }
}

fn server_transition_locks(
    spec: &crate::features::server::domain::ServerSpec,
) -> Vec<(ResourceKey, ResourceLockMode)> {
    let mut locks = vec![
        (ResourceKey::maintenance(), ResourceLockMode::Shared),
        (
            ResourceKey::new(ResourceKind::Server, spec.server_ref.to_string()),
            ResourceLockMode::Exclusive,
        ),
    ];
    if let Some(model_ref) = spec.local_model_ref() {
        locks.push((
            ResourceKey::new(ResourceKind::Model, model_ref.to_string()),
            ResourceLockMode::Shared,
        ));
        if let Some(capability) = spec.capability {
            locks.push((
                ResourceKey::new(
                    ResourceKind::ModelCapability,
                    format!("{model_ref}|{}", capability.required_model_capability()),
                ),
                ResourceLockMode::Shared,
            ));
        }
    }
    if let Some(cluster_ref) = spec.cluster_ref.as_ref() {
        locks.push((
            ResourceKey::new(ResourceKind::Cluster, cluster_ref.to_string()),
            ResourceLockMode::Shared,
        ));
    }
    locks
}
