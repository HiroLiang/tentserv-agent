//! Read-only route execution decisions for cluster server requests.

use crate::features::{
    cluster::domain::{
        ClusterLocalRouteExecutionTarget, ClusterRouteExecutionBlockerCode,
        ClusterRouteExecutionDecision, ClusterRouteReadinessStatus, ClusterRouteTarget,
    },
    model::{
        file_diagnostics::{
            model_file_diagnostics, model_file_diagnostics_block_execution,
            model_file_diagnostics_summary,
        },
        ports::{ModelCapabilityProofStore, ModelCatalogStore},
    },
};
use crate::foundation::{error::KernelResult, layout::RuntimeLayoutResolver};

use super::{
    common::{cluster_store_layout, model_store_layout},
    port::{ClusterRouteExecutionUseCase, ClusterRouteResolveRequest, ClusterRouteResolveResult},
    readiness::resolve_local_route_readiness,
};

/// Resolves one cached cluster definition route without mutating state.
pub struct StdClusterRouteExecutionUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    model_catalog: &'a dyn ModelCatalogStore,
    model_proofs: &'a dyn ModelCapabilityProofStore,
}

impl<'a> StdClusterRouteExecutionUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        model_catalog: &'a dyn ModelCatalogStore,
        model_proofs: &'a dyn ModelCapabilityProofStore,
    ) -> Self {
        Self {
            layout_resolver,
            model_catalog,
            model_proofs,
        }
    }
}

impl ClusterRouteExecutionUseCase for StdClusterRouteExecutionUseCase<'_> {
    fn resolve_cluster_route(
        &self,
        request: ClusterRouteResolveRequest,
    ) -> KernelResult<ClusterRouteResolveResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let cluster_store = cluster_store_layout(&layout);
        let model_store = model_store_layout(&layout);
        let Some(target) = request.definition.routes.get(&request.route) else {
            return Ok(ClusterRouteResolveResult {
                layout,
                decision: ClusterRouteExecutionDecision::Blocked {
                    route: request.route,
                    code: ClusterRouteExecutionBlockerCode::MissingRoute,
                    status: None,
                    description: format!(
                        "cluster `{}` does not declare route `{}`",
                        request.definition.cluster_ref, request.route
                    ),
                },
            });
        };

        let ClusterRouteTarget::LocalModel {
            model_ref,
            runtime_profile,
        } = target
        else {
            return Ok(ClusterRouteResolveResult {
                layout,
                decision: ClusterRouteExecutionDecision::Blocked {
                    route: request.route,
                    code: ClusterRouteExecutionBlockerCode::UnsupportedTarget,
                    status: None,
                    description: format!(
                        "cluster `{}` route `{}` uses a provider target, which cluster servers do not execute in this release",
                        request.definition.cluster_ref, request.route
                    ),
                },
            });
        };

        let readiness = resolve_local_route_readiness(
            request.route,
            model_ref.to_string(),
            runtime_profile,
            &request.definition.cluster_ref,
            &cluster_store,
            &model_store,
            self.model_catalog,
            self.model_proofs,
        );
        let allowed = readiness.status.is_ready()
            || (request.allow_unverified
                && matches!(
                    readiness.status,
                    ClusterRouteReadinessStatus::Unknown | ClusterRouteReadinessStatus::Stale
                ));
        if !allowed {
            let code = blocker_code_for_status(readiness.status);
            return Ok(ClusterRouteResolveResult {
                layout,
                decision: ClusterRouteExecutionDecision::Blocked {
                    route: request.route,
                    code,
                    status: Some(readiness.status),
                    description: readiness
                        .reason
                        .clone()
                        .unwrap_or_else(|| readiness.description.clone()),
                },
            });
        }

        let metadata = self
            .model_catalog
            .load_model_metadata(&model_store, model_ref)?;
        let diagnostics = model_file_diagnostics(&model_store, &metadata);
        if model_file_diagnostics_block_execution(&diagnostics) {
            return Ok(ClusterRouteResolveResult {
                layout,
                decision: ClusterRouteExecutionDecision::Blocked {
                    route: request.route,
                    code: ClusterRouteExecutionBlockerCode::Unavailable,
                    status: Some(ClusterRouteReadinessStatus::Unavailable),
                    description: format!(
                        "model files are incomplete: {}",
                        model_file_diagnostics_summary(&diagnostics)
                    ),
                },
            });
        }

        Ok(ClusterRouteResolveResult {
            layout,
            decision: ClusterRouteExecutionDecision::Ready(ClusterLocalRouteExecutionTarget {
                route: request.route,
                model_ref: model_ref.clone(),
                capability: request.route.server_capability(),
                runtime_profile: readiness.runtime_profile.effective.clone(),
            }),
        })
    }
}

fn blocker_code_for_status(
    status: ClusterRouteReadinessStatus,
) -> ClusterRouteExecutionBlockerCode {
    match status {
        ClusterRouteReadinessStatus::Stale => ClusterRouteExecutionBlockerCode::ProofStale,
        ClusterRouteReadinessStatus::Failed => ClusterRouteExecutionBlockerCode::ProofFailed,
        ClusterRouteReadinessStatus::Unsupported => ClusterRouteExecutionBlockerCode::Unsupported,
        ClusterRouteReadinessStatus::Unavailable => ClusterRouteExecutionBlockerCode::Unavailable,
        ClusterRouteReadinessStatus::Unknown
        | ClusterRouteReadinessStatus::AuthMissing
        | ClusterRouteReadinessStatus::AuthAttention => ClusterRouteExecutionBlockerCode::NotReady,
        ClusterRouteReadinessStatus::Ready
        | ClusterRouteReadinessStatus::Verified
        | ClusterRouteReadinessStatus::Supported => ClusterRouteExecutionBlockerCode::NotReady,
    }
}
