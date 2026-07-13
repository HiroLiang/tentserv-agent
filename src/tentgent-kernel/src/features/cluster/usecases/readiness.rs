//! Read-only cluster route readiness diagnostics.

use crate::features::{
    auth::{
        domain::{AuthEnvLoadPolicy, AuthKeyStatus, AuthSourceMode, AuthValidationState, Provider},
        usecases::{AuthStatusRequest, AuthStatusUseCase},
    },
    cloud::domain::{provider_capabilities, provider_supports},
    cluster::{
        domain::{
            ClusterInspection, ClusterReadinessAction, ClusterReadinessActionCode,
            ClusterReadinessDetail, ClusterReadinessReport, ClusterReadinessStatus, ClusterRef,
            ClusterRouteKey, ClusterRouteReadiness, ClusterRouteReadinessStatus,
            ClusterRouteTarget, ClusterRuntimeProfileReadiness, ClusterRuntimeProfileSource,
            ClusterStoreLayout,
        },
        ports::ClusterCatalogStore,
    },
    doctor::domain::{
        DoctorCheck, DoctorCheckCategory, DoctorCheckDetail, DoctorCheckStatus, DoctorNextAction,
    },
    model::{
        domain::{ModelCapability, ModelMetadata, ModelStoreLayout},
        ports::{ModelCapabilityProofStore, ModelCatalogStore},
        support_catalog::built_in_support_hints_for_model,
        support_status::{
            ModelSupportQuery, ModelSupportResolution, ModelSupportStatus,
            ModelSupportStatusResolver,
        },
    },
    server::profile::local_server_runtime_profile_for,
};
use crate::foundation::{
    error::KernelResult,
    layout::{RuntimeLayout, RuntimeLayoutResolver},
};

use super::{
    common::{
        auth_provider_for_cloud_provider, cloud_endpoint_capability_for_route,
        cluster_store_layout, model_store_layout, server_runtime_backend_for_format,
    },
    port::{
        ClusterReadinessInspectRequest, ClusterReadinessInspectResult, ClusterReadinessListRequest,
        ClusterReadinessListResult, ClusterReadinessUseCase,
    },
};

/// Standard read-only orchestration for cluster route readiness.
pub struct StdClusterReadinessUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn ClusterCatalogStore,
    model_catalog: &'a dyn ModelCatalogStore,
    model_proofs: &'a dyn ModelCapabilityProofStore,
    auth_status: &'a dyn AuthStatusUseCase,
}

impl<'a> StdClusterReadinessUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ClusterCatalogStore,
        model_catalog: &'a dyn ModelCatalogStore,
        model_proofs: &'a dyn ModelCapabilityProofStore,
        auth_status: &'a dyn AuthStatusUseCase,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            model_catalog,
            model_proofs,
            auth_status,
        }
    }
}

impl ClusterReadinessUseCase for StdClusterReadinessUseCase<'_> {
    fn inspect_cluster_readiness(
        &self,
        request: ClusterReadinessInspectRequest,
    ) -> KernelResult<ClusterReadinessInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let inspection = self.catalog.inspect_cluster(&store, &request.cluster_ref)?;
        let readiness = resolve_cluster_readiness(
            &layout,
            &store,
            &inspection,
            self.model_catalog,
            self.model_proofs,
            self.auth_status,
        );

        Ok(ClusterReadinessInspectResult {
            layout,
            store,
            inspection,
            readiness,
        })
    }

    fn list_cluster_readiness(
        &self,
        request: ClusterReadinessListRequest,
    ) -> KernelResult<ClusterReadinessListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = cluster_store_layout(&layout);
        let summaries = self.catalog.list_clusters(&store)?;
        let mut clusters = Vec::with_capacity(summaries.len());
        for summary in summaries {
            let inspection = self.catalog.inspect_cluster(&store, &summary.cluster_ref)?;
            let readiness = resolve_cluster_readiness(
                &layout,
                &store,
                &inspection,
                self.model_catalog,
                self.model_proofs,
                self.auth_status,
            );
            clusters.push(ClusterReadinessInspectResult {
                layout: layout.clone(),
                store: store.clone(),
                inspection,
                readiness,
            });
        }

        Ok(ClusterReadinessListResult {
            layout,
            store,
            clusters,
        })
    }
}

/// Maps cluster readiness reports into compact doctor checks.
pub fn cluster_readiness_doctor_checks<'a>(
    clusters: impl IntoIterator<Item = (&'a ClusterRef, &'a ClusterReadinessReport)>,
) -> Vec<DoctorCheck> {
    let clusters = clusters.into_iter().collect::<Vec<_>>();
    if clusters.is_empty() {
        return Vec::new();
    }

    let attention = clusters
        .iter()
        .filter(|(_, readiness)| readiness.status != ClusterReadinessStatus::Ready)
        .copied()
        .collect::<Vec<_>>();
    let status = if attention.is_empty() {
        DoctorCheckStatus::Pass
    } else {
        DoctorCheckStatus::Warn
    };
    let detail = if attention.is_empty() {
        format!("{}/{} cluster(s) ready", clusters.len(), clusters.len())
    } else {
        format!(
            "{}/{} cluster(s) need attention",
            attention.len(),
            clusters.len()
        )
    };

    let mut check = DoctorCheck::with_status(
        DoctorCheckCategory::Cluster,
        "cluster readiness",
        status,
        detail.clone(),
    )
    .with_description(detail)
    .with_flags(cluster_doctor_flags(
        clusters.iter().map(|(_, readiness)| *readiness),
    ))
    .with_details(
        attention
            .iter()
            .map(|(cluster_ref, readiness)| DoctorCheckDetail {
                name: cluster_ref.to_string(),
                description: cluster_readiness_summary(readiness),
                flags: readiness.flags.clone(),
            }),
    );

    for (cluster_ref, _) in attention {
        check = check.with_next_action(
            DoctorNextAction::command(
                format!("Inspect cluster {cluster_ref}"),
                format!("tentgent cluster inspect {cluster_ref}"),
            )
            .with_code(ClusterReadinessActionCode::InspectCluster.as_str()),
        );
    }

    vec![check]
}

fn cluster_readiness_summary(readiness: &ClusterReadinessReport) -> String {
    format!(
        "{}; {} ready, {} need attention",
        readiness.status.as_str(),
        readiness.ready_route_count,
        readiness.attention_route_count
    )
}

fn cluster_doctor_flags<'a>(
    reports: impl IntoIterator<Item = &'a ClusterReadinessReport>,
) -> Vec<String> {
    let mut flags = Vec::new();
    for report in reports {
        flags.extend(report.flags.iter().cloned());
        if report.status != ClusterReadinessStatus::Ready {
            flags.push("has-cluster-attention".to_string());
        }
    }
    dedupe(flags)
}

fn resolve_cluster_readiness(
    layout: &RuntimeLayout,
    store: &ClusterStoreLayout,
    inspection: &ClusterInspection,
    model_catalog: &dyn ModelCatalogStore,
    model_proofs: &dyn ModelCapabilityProofStore,
    auth_status: &dyn AuthStatusUseCase,
) -> ClusterReadinessReport {
    let model_store = model_store_layout(layout);
    let provider_statuses = provider_auth_statuses(inspection, auth_status);
    let mut routes = Vec::with_capacity(inspection.definition.routes.len());

    for (route, target) in &inspection.definition.routes {
        routes.push(resolve_route_readiness(
            *route,
            target,
            &inspection.definition.cluster_ref,
            store,
            &model_store,
            model_catalog,
            model_proofs,
            provider_statuses.as_ref(),
        ));
    }

    aggregate_readiness(routes)
}

fn provider_auth_statuses(
    inspection: &ClusterInspection,
    auth_status: &dyn AuthStatusUseCase,
) -> Result<Vec<AuthKeyStatus>, String> {
    let mut providers = Vec::new();
    for target in inspection.definition.routes.values() {
        if let ClusterRouteTarget::Provider { provider, .. } = target {
            let provider = auth_provider_for_cloud_provider(*provider);
            if !providers.contains(&provider) {
                providers.push(provider);
            }
        }
    }

    if providers.is_empty() {
        return Ok(Vec::new());
    }

    auth_status
        .status(AuthStatusRequest {
            providers,
            env_policy: AuthEnvLoadPolicy::CwdDotenvOverride,
            probe_keychain_presence: false,
        })
        .map(|report| report.statuses)
        .map_err(|err| err.to_string())
}

fn resolve_route_readiness(
    route: ClusterRouteKey,
    target: &ClusterRouteTarget,
    cluster_ref: &ClusterRef,
    store: &ClusterStoreLayout,
    model_store: &ModelStoreLayout,
    model_catalog: &dyn ModelCatalogStore,
    model_proofs: &dyn ModelCapabilityProofStore,
    provider_statuses: Result<&Vec<AuthKeyStatus>, &String>,
) -> ClusterRouteReadiness {
    match target {
        ClusterRouteTarget::LocalModel {
            model_ref,
            runtime_profile,
        } => resolve_local_route_readiness(
            route,
            model_ref.to_string(),
            runtime_profile,
            cluster_ref,
            store,
            model_store,
            model_catalog,
            model_proofs,
        ),
        ClusterRouteTarget::Provider {
            provider,
            provider_model,
        } => resolve_provider_route_readiness(route, *provider, provider_model, provider_statuses),
    }
}

pub(super) fn resolve_local_route_readiness(
    route: ClusterRouteKey,
    model_ref: String,
    configured_runtime_profile: &Option<
        crate::features::server::domain::ServerRuntimeProfileSelection,
    >,
    cluster_ref: &ClusterRef,
    store: &ClusterStoreLayout,
    model_store: &ModelStoreLayout,
    model_catalog: &dyn ModelCatalogStore,
    model_proofs: &dyn ModelCapabilityProofStore,
) -> ClusterRouteReadiness {
    let capability = route.model_capability();
    let metadata = match model_catalog.load_model_metadata(
        model_store,
        &crate::features::model::domain::ModelRef::parse(&model_ref)
            .expect("stored cluster model_ref should be valid"),
    ) {
        Ok(metadata) => metadata,
        Err(err) => {
            return unavailable_route(
                route,
                "local-model",
                model_ref,
                capability,
                format!("model metadata lookup failed: {err}"),
            );
        }
    };

    let backend =
        match server_runtime_backend_for_format(route.server_capability(), metadata.primary_format)
        {
            Ok(backend) => backend,
            Err(err) => {
                return unsupported_local_route(
                    route,
                    &metadata,
                    capability,
                    None,
                    format!("{err}"),
                );
            }
        };
    let mut flags = Vec::new();
    let mut details = Vec::new();
    let mut next_actions = Vec::new();
    let runtime_profile = local_runtime_profile_readiness(
        route,
        backend,
        configured_runtime_profile.clone(),
        cluster_ref,
        store,
        &mut flags,
        &mut details,
        &mut next_actions,
    );

    let mut query = ModelSupportQuery::from_metadata(&metadata, capability);
    if let Some(profile) = runtime_profile.effective.as_ref() {
        query = query.with_runtime_profile(profile.profile_id.clone(), profile.profile_version);
    }

    let proofs = match model_proofs.list_capability_proofs(model_store, &metadata.model_ref) {
        Ok(proofs) => proofs,
        Err(err) => {
            return unavailable_route(
                route,
                "local-model",
                metadata.model_ref.to_string(),
                capability,
                format!("proof lookup failed: {err}"),
            );
        }
    };

    let hints = built_in_support_hints_for_model(&metadata);
    let resolution = ModelSupportStatusResolver.resolve(&metadata, &query, &proofs, &hints);
    let status = route_status_for_support(resolution.status);
    if status.needs_attention() {
        flags.push(flag_for_route_status(status).to_string());
    }
    if let Some(reason) = resolution
        .failure_reason
        .as_deref()
        .or(resolution.stale_reason.as_deref())
    {
        details.push(ClusterReadinessDetail {
            name: route.to_string(),
            description: reason.to_string(),
            flags: vec![flag_for_route_status(status).to_string()],
        });
    }
    next_actions.extend(local_support_actions(&metadata, capability, &resolution));

    ClusterRouteReadiness {
        route,
        kind: "local-model".to_string(),
        target: metadata.model_ref.to_string(),
        capability,
        backend: Some(query.backend),
        runtime_profile,
        status,
        support_status: Some(resolution.status),
        evidence: Some(resolution.evidence),
        description: resolution.reason,
        reason: resolution.failure_reason.or(resolution.stale_reason),
        flags: dedupe(flags),
        details,
        next_actions,
    }
}

fn local_runtime_profile_readiness(
    route: ClusterRouteKey,
    backend: crate::features::server::domain::ServerRuntimeBackend,
    configured: Option<crate::features::server::domain::ServerRuntimeProfileSelection>,
    cluster_ref: &ClusterRef,
    store: &ClusterStoreLayout,
    flags: &mut Vec<String>,
    details: &mut Vec<ClusterReadinessDetail>,
    next_actions: &mut Vec<ClusterReadinessAction>,
) -> ClusterRuntimeProfileReadiness {
    if let Some(configured) = configured {
        return ClusterRuntimeProfileReadiness {
            configured: Some(configured.clone()),
            effective: Some(configured),
            source: ClusterRuntimeProfileSource::Configured,
        };
    }

    let inferred = local_server_runtime_profile_for(route.server_capability(), backend)
        .map(|profile| profile.selection);
    if let Some(inferred) = inferred {
        flags.push("runtime-profile-inferred".to_string());
        details.push(ClusterReadinessDetail {
            name: format!("{route} runtime profile"),
            description: format!(
                "runtime profile `{}` was inferred; add it to `{}` for reproducible cluster definitions",
                inferred.label(),
                store.cluster_definition_path(cluster_ref.as_str()).display()
            ),
            flags: vec!["runtime-profile-inferred".to_string()],
        });
        next_actions.push(ClusterReadinessAction {
            code: ClusterReadinessActionCode::UpdateClusterDefinition,
            label: "Update cluster definition".to_string(),
            command: None,
            description: Some(format!(
                "Add [routes.{route}.runtime_profile] profile_id = \"{}\" and profile_version = {}",
                inferred.profile_id, inferred.profile_version
            )),
        });
        ClusterRuntimeProfileReadiness {
            configured: None,
            effective: Some(inferred),
            source: ClusterRuntimeProfileSource::Inferred,
        }
    } else {
        ClusterRuntimeProfileReadiness {
            configured: None,
            effective: None,
            source: ClusterRuntimeProfileSource::None,
        }
    }
}

fn resolve_provider_route_readiness(
    route: ClusterRouteKey,
    provider: crate::features::server::domain::CloudProvider,
    provider_model: &str,
    provider_statuses: Result<&Vec<AuthKeyStatus>, &String>,
) -> ClusterRouteReadiness {
    let capability = route.model_capability();
    let auth_provider = auth_provider_for_cloud_provider(provider);
    let target = format!("{}:{provider_model}", provider.as_str());
    let runtime_profile = ClusterRuntimeProfileReadiness {
        configured: None,
        effective: None,
        source: ClusterRuntimeProfileSource::None,
    };

    let Some(cloud_capability) = cloud_endpoint_capability_for_route(route) else {
        return ClusterRouteReadiness {
            route,
            kind: "provider".to_string(),
            target,
            capability,
            backend: None,
            runtime_profile,
            status: ClusterRouteReadinessStatus::Unsupported,
            support_status: None,
            evidence: None,
            description: format!("provider route `{route}` is not supported for cloud providers"),
            reason: Some(format!(
                "supported cloud capabilities are {}",
                provider_capabilities(auth_provider)
                    .iter()
                    .map(|capability| capability.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            flags: vec!["unsupported-provider-route".to_string()],
            details: Vec::new(),
            next_actions: vec![choose_supported_route_action()],
        };
    };

    if !provider_supports(auth_provider, cloud_capability) {
        return ClusterRouteReadiness {
            route,
            kind: "provider".to_string(),
            target,
            capability,
            backend: Some(provider.as_str().to_string()),
            runtime_profile,
            status: ClusterRouteReadinessStatus::Unsupported,
            support_status: None,
            evidence: None,
            description: format!("provider `{provider}` does not support route `{route}`"),
            reason: Some(format!(
                "supported cloud capabilities are {}",
                provider_capabilities(auth_provider)
                    .iter()
                    .map(|capability| capability.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            flags: vec!["unsupported-provider-route".to_string()],
            details: Vec::new(),
            next_actions: vec![choose_supported_route_action()],
        };
    }

    let status = match provider_statuses {
        Ok(statuses) => statuses
            .iter()
            .find(|status| status.provider == auth_provider),
        Err(err) => {
            return unavailable_provider_route(route, provider, provider_model, capability, err);
        }
    };
    let Some(status) = status else {
        return unavailable_provider_route(
            route,
            provider,
            provider_model,
            capability,
            "provider auth status missing",
        );
    };

    let (readiness, description, reason, flags, actions) = provider_auth_readiness(status);

    ClusterRouteReadiness {
        route,
        kind: "provider".to_string(),
        target,
        capability,
        backend: Some(provider.as_str().to_string()),
        runtime_profile,
        status: readiness,
        support_status: None,
        evidence: None,
        description,
        reason,
        flags,
        details: Vec::new(),
        next_actions: actions,
    }
}

fn provider_auth_readiness(
    status: &AuthKeyStatus,
) -> (
    ClusterRouteReadinessStatus,
    String,
    Option<String>,
    Vec<String>,
    Vec<ClusterReadinessAction>,
) {
    if status.preference.source_mode == AuthSourceMode::None {
        return (
            ClusterRouteReadinessStatus::AuthMissing,
            format!("{} auth is disabled", status.provider.display_name()),
            Some("provider auth mode is none".to_string()),
            vec!["provider-auth-disabled".to_string()],
            vec![set_provider_auth_action(status.provider)],
        );
    }

    if status.effective_source.is_none() {
        return (
            ClusterRouteReadinessStatus::AuthMissing,
            format!(
                "{} auth is not configured locally",
                status.provider.display_name()
            ),
            Some(format!(
                "no effective {} auth source was found without reading secrets",
                status.provider.display_name()
            )),
            vec!["missing-auth".to_string()],
            vec![set_provider_auth_action(status.provider)],
        );
    }

    match &status.validation {
        AuthValidationState::Invalid { reason } => (
            ClusterRouteReadinessStatus::AuthAttention,
            format!(
                "{} auth validation needs attention",
                status.provider.display_name()
            ),
            Some(reason.clone()),
            vec!["auth-validation-invalid".to_string()],
            vec![set_provider_auth_action(status.provider)],
        ),
        AuthValidationState::Unknown { reason } => (
            ClusterRouteReadinessStatus::AuthAttention,
            format!(
                "{} auth validation is unknown",
                status.provider.display_name()
            ),
            Some(reason.clone()),
            vec!["auth-validation-unknown".to_string()],
            vec![set_provider_auth_action(status.provider)],
        ),
        AuthValidationState::Missing => (
            ClusterRouteReadinessStatus::AuthMissing,
            format!("{} auth is missing", status.provider.display_name()),
            None,
            vec!["missing-auth".to_string()],
            vec![set_provider_auth_action(status.provider)],
        ),
        AuthValidationState::NotChecked | AuthValidationState::Verified => (
            ClusterRouteReadinessStatus::Ready,
            format!(
                "{} auth source is configured; validation is {}",
                status.provider.display_name(),
                status.validation.summary()
            ),
            None,
            Vec::new(),
            Vec::new(),
        ),
    }
}

fn unavailable_route(
    route: ClusterRouteKey,
    kind: &str,
    target: String,
    capability: ModelCapability,
    reason: String,
) -> ClusterRouteReadiness {
    ClusterRouteReadiness {
        route,
        kind: kind.to_string(),
        target,
        capability,
        backend: None,
        runtime_profile: ClusterRuntimeProfileReadiness {
            configured: None,
            effective: None,
            source: ClusterRuntimeProfileSource::Unavailable,
        },
        status: ClusterRouteReadinessStatus::Unavailable,
        support_status: None,
        evidence: None,
        description: "route readiness could not be computed".to_string(),
        reason: Some(reason),
        flags: vec!["readiness-unavailable".to_string()],
        details: Vec::new(),
        next_actions: vec![choose_supported_route_action()],
    }
}

fn unavailable_provider_route(
    route: ClusterRouteKey,
    provider: crate::features::server::domain::CloudProvider,
    provider_model: &str,
    capability: ModelCapability,
    reason: impl Into<String>,
) -> ClusterRouteReadiness {
    unavailable_route(
        route,
        "provider",
        format!("{}:{provider_model}", provider.as_str()),
        capability,
        reason.into(),
    )
}

fn unsupported_local_route(
    route: ClusterRouteKey,
    metadata: &ModelMetadata,
    capability: ModelCapability,
    backend: Option<String>,
    reason: String,
) -> ClusterRouteReadiness {
    ClusterRouteReadiness {
        route,
        kind: "local-model".to_string(),
        target: metadata.model_ref.to_string(),
        capability,
        backend,
        runtime_profile: ClusterRuntimeProfileReadiness {
            configured: None,
            effective: None,
            source: ClusterRuntimeProfileSource::Unavailable,
        },
        status: ClusterRouteReadinessStatus::Unsupported,
        support_status: Some(ModelSupportStatus::Unsupported),
        evidence: None,
        description: "local route target is unsupported".to_string(),
        reason: Some(reason),
        flags: vec!["unsupported-route-target".to_string()],
        details: Vec::new(),
        next_actions: vec![
            inspect_model_action(&metadata.short_ref),
            choose_supported_route_action(),
        ],
    }
}

fn route_status_for_support(status: ModelSupportStatus) -> ClusterRouteReadinessStatus {
    match status {
        ModelSupportStatus::Verified => ClusterRouteReadinessStatus::Verified,
        ModelSupportStatus::Supported => ClusterRouteReadinessStatus::Supported,
        ModelSupportStatus::Unknown => ClusterRouteReadinessStatus::Unknown,
        ModelSupportStatus::Stale => ClusterRouteReadinessStatus::Stale,
        ModelSupportStatus::Failed => ClusterRouteReadinessStatus::Failed,
        ModelSupportStatus::Unsupported => ClusterRouteReadinessStatus::Unsupported,
    }
}

fn flag_for_route_status(status: ClusterRouteReadinessStatus) -> &'static str {
    match status {
        ClusterRouteReadinessStatus::Failed => "failed-proof",
        ClusterRouteReadinessStatus::Stale => "stale-proof",
        ClusterRouteReadinessStatus::Unsupported => "unsupported-route",
        ClusterRouteReadinessStatus::Unknown => "unknown-support",
        ClusterRouteReadinessStatus::AuthMissing => "missing-auth",
        ClusterRouteReadinessStatus::AuthAttention => "auth-attention",
        ClusterRouteReadinessStatus::Unavailable => "readiness-unavailable",
        ClusterRouteReadinessStatus::Ready
        | ClusterRouteReadinessStatus::Verified
        | ClusterRouteReadinessStatus::Supported => "ready",
    }
}

fn local_support_actions(
    metadata: &ModelMetadata,
    capability: ModelCapability,
    resolution: &ModelSupportResolution,
) -> Vec<ClusterReadinessAction> {
    match resolution.status {
        ModelSupportStatus::Verified | ModelSupportStatus::Supported => Vec::new(),
        ModelSupportStatus::Unknown => vec![verify_model_capability_action(
            &metadata.short_ref,
            capability,
        )],
        ModelSupportStatus::Failed => vec![
            clear_model_proof_action(&metadata.short_ref, capability),
            verify_model_capability_action(&metadata.short_ref, capability),
        ],
        ModelSupportStatus::Stale => vec![
            verify_model_capability_action(&metadata.short_ref, capability),
            clear_model_proof_action(&metadata.short_ref, capability),
        ],
        ModelSupportStatus::Unsupported if !metadata.supports_capability(capability) => vec![
            inspect_model_action(&metadata.short_ref),
            choose_supported_route_action(),
        ],
        ModelSupportStatus::Unsupported => {
            vec![
                inspect_model_action(&metadata.short_ref),
                choose_supported_route_action(),
            ]
        }
    }
}

fn verify_model_capability_action(
    model_ref: &str,
    capability: ModelCapability,
) -> ClusterReadinessAction {
    ClusterReadinessAction {
        code: ClusterReadinessActionCode::VerifyModelCapability,
        label: "Verify model capability".to_string(),
        command: Some(format!(
            "tentgent model capability verify {model_ref} {}",
            capability.as_str()
        )),
        description: Some("Record fresh local support evidence for this route tuple".to_string()),
    }
}

fn clear_model_proof_action(
    model_ref: &str,
    capability: ModelCapability,
) -> ClusterReadinessAction {
    ClusterReadinessAction {
        code: ClusterReadinessActionCode::ClearModelProof,
        label: "Clear model proof".to_string(),
        command: Some(format!(
            "tentgent model capability proof clear {model_ref} {}",
            capability.as_str()
        )),
        description: Some(
            "Clear stale or failed local support evidence before retrying".to_string(),
        ),
    }
}

fn inspect_model_action(model_ref: &str) -> ClusterReadinessAction {
    ClusterReadinessAction {
        code: ClusterReadinessActionCode::InspectModel,
        label: "Inspect model".to_string(),
        command: Some(format!("tentgent model inspect {model_ref}")),
        description: Some(
            "Inspect model metadata, files, capabilities, and support evidence".to_string(),
        ),
    }
}

fn set_provider_auth_action(provider: Provider) -> ClusterReadinessAction {
    ClusterReadinessAction {
        code: ClusterReadinessActionCode::SetProviderAuth,
        label: format!("Set {} auth", provider.display_name()),
        command: Some(format!("tentgent auth {} set", provider.cli_name())),
        description: Some(format!(
            "Configure {} auth before using this provider route",
            provider.display_name()
        )),
    }
}

fn choose_supported_route_action() -> ClusterReadinessAction {
    ClusterReadinessAction {
        code: ClusterReadinessActionCode::ChooseSupportedRouteTarget,
        label: "Choose supported route target".to_string(),
        command: None,
        description: Some(
            "Update the cluster route to a model, provider, capability, or backend tuple that is supported"
                .to_string(),
        ),
    }
}

fn aggregate_readiness(routes: Vec<ClusterRouteReadiness>) -> ClusterReadinessReport {
    let ready_route_count = routes
        .iter()
        .filter(|route| route.status.is_ready())
        .count();
    let attention_route_count = routes.len().saturating_sub(ready_route_count);
    let status = if routes.is_empty()
        || routes
            .iter()
            .all(|route| route.status == ClusterRouteReadinessStatus::Unavailable)
    {
        ClusterReadinessStatus::Unknown
    } else if ready_route_count == routes.len() {
        ClusterReadinessStatus::Ready
    } else if ready_route_count > 0 {
        ClusterReadinessStatus::Partial
    } else {
        ClusterReadinessStatus::Blocked
    };

    let mut flags = Vec::new();
    for route in &routes {
        flags.extend(route.flags.iter().cloned());
        if route.status.needs_attention() {
            flags.push("has-attention-route".to_string());
        }
    }
    if status == ClusterReadinessStatus::Partial {
        flags.push("partial-cluster".to_string());
    }

    let details = routes
        .iter()
        .filter(|route| route.status.needs_attention() || !route.details.is_empty())
        .map(|route| ClusterReadinessDetail {
            name: route.route.to_string(),
            description: route
                .reason
                .clone()
                .unwrap_or_else(|| route.description.clone()),
            flags: route.flags.clone(),
        })
        .collect();

    ClusterReadinessReport {
        status,
        ready_route_count,
        attention_route_count,
        flags: dedupe(flags),
        routes,
        details,
    }
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    deduped
}
