use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::cluster::domain::{
    ClusterInspection, ClusterRouteTarget, ClusterSummary,
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
    pub routes: Vec<ClusterRouteItem>,
    pub home_dir: String,
    pub cluster_dir: String,
    pub definition_path: String,
}

#[derive(Debug, Serialize)]
pub struct ClusterRouteItem {
    pub route: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_profile: Option<ServerRuntimeProfileSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_model: Option<String>,
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
    ClusterInspectionItem {
        cluster_ref: inspection.definition.cluster_ref.to_string(),
        schema_version: inspection.definition.schema_version,
        routes: inspection
            .definition
            .routes
            .into_iter()
            .map(|(route, target)| match target {
                ClusterRouteTarget::LocalModel {
                    model_ref,
                    runtime_profile,
                } => ClusterRouteItem {
                    route: route.to_string(),
                    kind: "local-model".to_string(),
                    model_ref: Some(model_ref.to_string()),
                    runtime_profile,
                    provider: None,
                    provider_model: None,
                },
                ClusterRouteTarget::Provider {
                    provider,
                    provider_model,
                } => ClusterRouteItem {
                    route: route.to_string(),
                    kind: "provider".to_string(),
                    model_ref: None,
                    runtime_profile: None,
                    provider: Some(provider.to_string()),
                    provider_model: Some(provider_model),
                },
            })
            .collect(),
        home_dir: path_string(&inspection.home_dir),
        cluster_dir: path_string(&inspection.cluster_dir),
        definition_path: path_string(&inspection.definition_path),
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
