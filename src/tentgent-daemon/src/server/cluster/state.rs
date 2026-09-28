use tentgent_kernel::{
    features::{
        cluster::{
            domain::{
                ClusterDefinition, ClusterRef, ClusterRouteExecutionDecision, ClusterRouteKey,
            },
            usecases::{
                ClusterRouteExecutionUseCase, ClusterRouteResolveRequest,
                StdClusterRouteExecutionUseCase,
            },
        },
        model::infra::{FileModelCapabilityProofStore, FileModelCatalogStore},
        runtime::infra::{
            ModelRuntimeDaemonLaunchPolicy, ModelRuntimeDaemonSupervisor,
            StdRuntimeExecutableResolver,
        },
        runtime_ownership::RuntimeExecutionIdentity,
        server::options::LoadMode,
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput},
};

use crate::server::local::{LocalServerRuntimeConfig, LocalServerState};

use super::leases::{RouteGenerationManager, RouteRequestLease};
use super::{cache::ClusterDefinitionCache, error::ClusterServerError};

#[derive(Debug, Clone)]
pub struct ClusterServerRuntimeConfig {
    pub server_ref: String,
    pub cluster_ref: ClusterRef,
    pub host: String,
    pub port: u16,
    pub runtime_home: Option<std::path::PathBuf>,
    pub runtime_idle_seconds: u64,
    pub model_idle_seconds: u64,
    pub allow_unverified: bool,
    pub load_mode: LoadMode,
}

#[derive(Clone)]
pub(super) struct ClusterServerState {
    pub(super) config: ClusterServerRuntimeConfig,
    pub(super) layout: RuntimeLayout,
    pub(super) runtime: tentgent_kernel::features::runtime::domain::PythonRuntimeLayout,
    pub(super) executable_resolver: StdRuntimeExecutableResolver,
    pub(super) supervisor: ModelRuntimeDaemonSupervisor,
    pub(super) client: reqwest::Client,
    pub(super) launch_policy: ModelRuntimeDaemonLaunchPolicy,
    pub(super) definitions: ClusterDefinitionCache,
    pub(super) routes: RouteGenerationManager,
    pub(super) startup: super::startup::ClusterStartupState,
}

pub(super) struct ResolvedClusterRoute {
    pub(super) local: LocalServerState,
    pub(super) lease: RouteRequestLease,
}

pub(super) struct PreparedClusterRoute {
    pub(super) route: ClusterRouteKey,
    pub(super) identity: RuntimeExecutionIdentity,
    pub(super) local: LocalServerState,
}

impl ClusterServerState {
    pub(super) fn resolve_local_state(
        &self,
        route: ClusterRouteKey,
    ) -> Result<ResolvedClusterRoute, ClusterServerError> {
        if !self.startup.is_ready() {
            return Err(ClusterServerError::starting());
        }
        let snapshot = self.definitions.current()?;
        self.routes.reconcile_definition(&snapshot.hash);
        let prepared = self.prepare_route(route, &snapshot.definition)?;
        let lease = self
            .routes
            .acquire(route, &snapshot.hash, prepared.identity)?;
        Ok(ResolvedClusterRoute {
            local: prepared.local,
            lease,
        })
    }

    pub(super) fn prepare_route(
        &self,
        route: ClusterRouteKey,
        definition: &ClusterDefinition,
    ) -> Result<PreparedClusterRoute, ClusterServerError> {
        let catalog = FileModelCatalogStore;
        let proofs = FileModelCapabilityProofStore;
        let resolver = StdClusterRouteExecutionUseCase::new(
            &tentgent_kernel::foundation::layout::StdRuntimeLayoutResolver,
            &catalog,
            &proofs,
        );
        let result = resolver
            .resolve_cluster_route(ClusterRouteResolveRequest {
                layout: RuntimeLayoutInput {
                    mode: LayoutResolveMode::ReadOnly,
                    home_dir: Some(self.layout.home_dir.clone()),
                    data_root_dir: Some(self.layout.data_root_dir.clone()),
                },
                definition: definition.clone(),
                route,
                allow_unverified: self.config.allow_unverified,
            })
            .map_err(|err| ClusterServerError::route_unavailable(err.to_string()))?;
        let ClusterRouteExecutionDecision::Ready(target) = result.decision else {
            return Err(ClusterServerError::from_decision(result.decision));
        };

        let identity = tentgent_kernel::features::runtime_ownership::RuntimeExecutionIdentity::model_bound(
            target.model_ref.to_string(),
            tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::from_model_capability(
                target.capability.required_model_capability(),
            ),
            target.runtime_profile.as_ref(),
        );
        Ok(PreparedClusterRoute {
            route,
            identity,
            local: LocalServerState {
                config: LocalServerRuntimeConfig {
                    server_ref: self.config.server_ref.clone(),
                    capability: target.capability,
                    model_ref: target.model_ref.to_string(),
                    runtime_profile: target.runtime_profile.map(|profile| profile.label()),
                    host: self.config.host.clone(),
                    port: self.config.port,
                    runtime_home: Some(self.layout.home_dir.clone()),
                    runtime_idle_seconds: self.config.runtime_idle_seconds,
                    model_idle_seconds: self.config.model_idle_seconds,
                    load_mode: tentgent_kernel::features::server::options::LoadMode::Lazy,
                },
                layout: self.layout.clone(),
                runtime: self.runtime.clone(),
                executable_resolver: self.executable_resolver,
                supervisor: self.supervisor.clone(),
                client: self.client.clone(),
                launch_policy: self.launch_policy.clone(),
                readiness: super::super::local::startup::StartupReadiness::new(
                    tentgent_kernel::features::server::options::LoadMode::Lazy,
                ),
            },
        })
    }
}
