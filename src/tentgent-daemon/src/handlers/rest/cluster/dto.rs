use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::cluster::domain::{
    ClusterInspection, ClusterReadinessAction, ClusterReadinessDetail, ClusterReadinessReport,
    ClusterRouteReadiness, ClusterRouteTarget, ClusterRuntimeProfileReadiness, ClusterSummary,
};
use tentgent_kernel::features::server::domain::ServerRuntimeProfileSelection;

#[derive(Debug, Serialize)]
pub struct ClustersResponse {
    pub clusters: Vec<ClusterSummaryItem>,
}

#[derive(Debug, Serialize)]
pub struct ClusterResponse {
    pub cluster: ClusterInspectionItem,
}

#[derive(Debug, Serialize)]
pub struct ClusterRemoveResponse {
    pub removed: ClusterRemovedItem,
    pub cluster: ClusterInspectionItem,
}

#[derive(Debug, Serialize)]
pub struct ClusterSummaryItem {
    pub cluster_ref: String,
    pub routes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ClusterInspectionItem {
    pub cluster_ref: String,
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ClusterReadinessItem>,
    pub routes: Vec<ClusterRouteItem>,
    pub home_dir: String,
    pub cluster_dir: String,
    pub definition_path: String,
}

#[derive(Debug, Serialize)]
pub struct ClusterRouteItem {
    pub route: String,
    pub kind: String,
    pub capability: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    /// Deprecated legacy configured runtime profile field. Use
    /// `runtime_profile_readiness` for configured/effective profile details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_profile: Option<ServerRuntimeProfileSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_profile_readiness: Option<ClusterRuntimeProfileReadinessItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ClusterRouteReadinessItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<ClusterReadinessActionItem>,
}

#[derive(Debug, Serialize)]
pub struct ClusterReadinessItem {
    pub status: String,
    pub ready_route_count: usize,
    pub attention_route_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ClusterReadinessDetailItem>,
}

#[derive(Debug, Serialize)]
pub struct ClusterRouteReadinessItem {
    pub status: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ClusterReadinessDetailItem>,
}

#[derive(Debug, Serialize)]
pub struct ClusterRuntimeProfileReadinessItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured: Option<ServerRuntimeProfileSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective: Option<ServerRuntimeProfileSelection>,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct ClusterReadinessDetailItem {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ClusterReadinessActionItem {
    pub code: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClusterRemovedItem {
    pub kind: &'static str,
    pub cluster_ref: String,
    pub cluster_dir: String,
}

pub fn cluster_summary_item(summary: ClusterSummary) -> ClusterSummaryItem {
    ClusterSummaryItem {
        cluster_ref: summary.cluster_ref.to_string(),
        routes: summary
            .route_keys
            .into_iter()
            .map(|route| route.as_str().to_string())
            .collect(),
    }
}

pub fn cluster_inspection_item(inspection: ClusterInspection) -> ClusterInspectionItem {
    cluster_inspection_item_with_readiness(inspection, None)
}

pub fn cluster_inspection_item_with_readiness(
    inspection: ClusterInspection,
    readiness: Option<ClusterReadinessReport>,
) -> ClusterInspectionItem {
    ClusterInspectionItem {
        cluster_ref: inspection.definition.cluster_ref.to_string(),
        schema_version: inspection.definition.schema_version,
        readiness: readiness.as_ref().map(cluster_readiness_item),
        routes: inspection
            .definition
            .routes
            .into_iter()
            .map(|(route, target)| match target {
                ClusterRouteTarget::LocalModel {
                    model_ref,
                    runtime_profile,
                } => {
                    let route_readiness = readiness.as_ref().and_then(|readiness| {
                        readiness.routes.iter().find(|item| item.route == route)
                    });
                    ClusterRouteItem {
                        route: route.to_string(),
                        kind: "local-model".to_string(),
                        capability: route.model_capability().as_str().to_string(),
                        model_ref: Some(model_ref.to_string()),
                        runtime_profile,
                        runtime_profile_readiness: route_readiness.map(|readiness| {
                            runtime_profile_readiness_item(&readiness.runtime_profile)
                        }),
                        provider: None,
                        provider_model: None,
                        backend: route_readiness.and_then(|readiness| readiness.backend.clone()),
                        readiness: route_readiness.map(cluster_route_readiness_item),
                        next_actions: route_readiness
                            .map(|readiness| {
                                readiness
                                    .next_actions
                                    .iter()
                                    .map(cluster_readiness_action_item)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    }
                }
                ClusterRouteTarget::Provider {
                    provider,
                    provider_model,
                } => {
                    let route_readiness = readiness.as_ref().and_then(|readiness| {
                        readiness.routes.iter().find(|item| item.route == route)
                    });
                    ClusterRouteItem {
                        route: route.to_string(),
                        kind: "provider".to_string(),
                        capability: route.model_capability().as_str().to_string(),
                        model_ref: None,
                        runtime_profile: None,
                        runtime_profile_readiness: route_readiness.map(|readiness| {
                            runtime_profile_readiness_item(&readiness.runtime_profile)
                        }),
                        provider: Some(provider.to_string()),
                        provider_model: Some(provider_model),
                        backend: route_readiness.and_then(|readiness| readiness.backend.clone()),
                        readiness: route_readiness.map(cluster_route_readiness_item),
                        next_actions: route_readiness
                            .map(|readiness| {
                                readiness
                                    .next_actions
                                    .iter()
                                    .map(cluster_readiness_action_item)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    }
                }
            })
            .collect(),
        home_dir: path_string(&inspection.home_dir),
        cluster_dir: path_string(&inspection.cluster_dir),
        definition_path: path_string(&inspection.definition_path),
    }
}

fn cluster_readiness_item(readiness: &ClusterReadinessReport) -> ClusterReadinessItem {
    ClusterReadinessItem {
        status: readiness.status.as_str().to_string(),
        ready_route_count: readiness.ready_route_count,
        attention_route_count: readiness.attention_route_count,
        flags: readiness.flags.clone(),
        details: readiness
            .details
            .iter()
            .map(cluster_readiness_detail_item)
            .collect(),
    }
}

fn cluster_route_readiness_item(readiness: &ClusterRouteReadiness) -> ClusterRouteReadinessItem {
    ClusterRouteReadinessItem {
        status: readiness.status.as_str().to_string(),
        description: readiness.description.clone(),
        reason: readiness.reason.clone(),
        support_status: readiness
            .support_status
            .map(|status| status.as_str().to_string()),
        evidence: readiness
            .evidence
            .map(|evidence| evidence.as_str().to_string()),
        flags: readiness.flags.clone(),
        details: readiness
            .details
            .iter()
            .map(cluster_readiness_detail_item)
            .collect(),
    }
}

fn runtime_profile_readiness_item(
    readiness: &ClusterRuntimeProfileReadiness,
) -> ClusterRuntimeProfileReadinessItem {
    ClusterRuntimeProfileReadinessItem {
        configured: readiness.configured.clone(),
        effective: readiness.effective.clone(),
        source: readiness.source.as_str().to_string(),
    }
}

fn cluster_readiness_detail_item(detail: &ClusterReadinessDetail) -> ClusterReadinessDetailItem {
    ClusterReadinessDetailItem {
        name: detail.name.clone(),
        description: detail.description.clone(),
        flags: detail.flags.clone(),
    }
}

fn cluster_readiness_action_item(action: &ClusterReadinessAction) -> ClusterReadinessActionItem {
    ClusterReadinessActionItem {
        code: action.code.as_str().to_string(),
        label: action.label.clone(),
        command: action.command.clone(),
        description: action.description.clone(),
    }
}

pub fn cluster_remove_response(
    outcome: tentgent_kernel::features::cluster::domain::ClusterRemoveOutcome,
) -> ClusterRemoveResponse {
    let removed = ClusterRemovedItem {
        kind: "cluster",
        cluster_ref: outcome.inspection.definition.cluster_ref.to_string(),
        cluster_dir: path_string(&outcome.inspection.cluster_dir),
    };
    ClusterRemoveResponse {
        cluster: cluster_inspection_item(outcome.inspection),
        removed,
    }
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}
