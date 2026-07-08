mod dto;

use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::{
    features::cluster::{
        domain::{ClusterDefinition, ClusterRef},
        usecases::{
            ClusterApplyDefinitionRequest, ClusterListRequest, ClusterReadinessInspectRequest,
            ClusterReadinessUseCase, ClusterRemoveRequest, ClusterSpecUseCase,
        },
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

use dto::{
    cluster_inspection_item, cluster_remove_response, cluster_summary_item, ClusterResponse,
    ClustersResponse,
};

pub async fn list(State(state): State<RestState>) -> Result<Json<ClustersResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .cluster_usecase()
        .list_clusters(ClusterListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(cluster_error)?;

    Ok(Json(ClustersResponse {
        clusters: result
            .clusters
            .into_iter()
            .map(cluster_summary_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(cluster_ref): Path<String>,
) -> Result<Json<ClusterResponse>, RestError> {
    let cluster_ref = parse_cluster_ref(&cluster_ref)?;
    let result = state
        .app()
        .services()
        .kernel()
        .cluster_readiness_usecase()
        .inspect_cluster_readiness(ClusterReadinessInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            cluster_ref,
        })
        .map_err(cluster_error)?;

    Ok(Json(ClusterResponse {
        cluster: dto::cluster_inspection_item_with_readiness(
            result.inspection,
            Some(result.readiness),
        ),
    }))
}

pub async fn apply(
    State(state): State<RestState>,
    Path(cluster_ref): Path<String>,
    Json(definition): Json<ClusterDefinition>,
) -> Result<Json<ClusterResponse>, RestError> {
    let cluster_ref = parse_cluster_ref(&cluster_ref)?;
    let result = state
        .app()
        .services()
        .kernel()
        .cluster_usecase()
        .apply_cluster_definition(ClusterApplyDefinitionRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            cluster_ref,
            definition,
        })
        .map_err(cluster_error)?;

    Ok(Json(ClusterResponse {
        cluster: cluster_inspection_item(result.inspection),
    }))
}

pub async fn remove(
    State(state): State<RestState>,
    Path(cluster_ref): Path<String>,
) -> Result<Json<dto::ClusterRemoveResponse>, RestError> {
    let cluster_ref = parse_cluster_ref(&cluster_ref)?;
    let result = state
        .app()
        .services()
        .kernel()
        .cluster_usecase()
        .remove_cluster(ClusterRemoveRequest {
            layout: state.app().layout_input(LayoutResolveMode::Create),
            cluster_ref,
        })
        .map_err(cluster_error)?;

    Ok(Json(cluster_remove_response(result.outcome)))
}

fn parse_cluster_ref(value: &str) -> Result<ClusterRef, RestError> {
    ClusterRef::parse(value).map_err(|err| {
        RestError::bad_request("bad_request", format!("invalid cluster reference: {err}"))
    })
}

fn cluster_error(error: KernelError) -> RestError {
    match error {
        KernelError::UnsupportedTarget(message) => RestError::bad_request("bad_request", message),
        KernelError::ServerStoreUnavailable(message) => {
            RestError::store_lookup("cluster_store_failed", message)
        }
        other => RestError::kernel("cluster_failed", other),
    }
}
