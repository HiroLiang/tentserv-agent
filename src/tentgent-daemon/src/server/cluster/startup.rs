use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use tentgent_kernel::features::{
    cluster::domain::{ClusterRouteKey, ClusterRouteTarget},
    runtime::infra::ModelRuntimePreloadFailureKind,
    server::options::LoadMode,
};

use super::{
    cache::ClusterDefinitionSnapshot,
    error::ClusterServerError,
    leases::RouteRequestLease,
    state::{ClusterServerState, PreparedClusterRoute},
};
use crate::server::local::{capability::ensure_model_endpoint, startup::preload_and_record};

#[derive(Clone)]
pub(super) struct ClusterStartupState {
    pub(super) snapshot: ClusterDefinitionSnapshot,
    ready: Arc<AtomicBool>,
}

impl ClusterStartupState {
    pub(super) fn new(snapshot: ClusterDefinitionSnapshot, mode: LoadMode) -> Self {
        Self {
            snapshot,
            ready: Arc::new(AtomicBool::new(mode == LoadMode::Lazy)),
        }
    }
    pub(super) fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    pub(super) fn mark_ready(&self) {
        self.ready.store(true, Ordering::Release);
    }
}

pub(super) async fn admit_ready_request(
    State(state): State<ClusterServerState>,
    request: Request,
    next: Next,
) -> Result<Response, ClusterServerError> {
    if request.uri().path() != "/healthz" && !state.startup.is_ready() {
        return Err(ClusterServerError::starting());
    }
    Ok(next.run(request).await)
}

pub(super) async fn prepare_cluster_startup(
    state: &ClusterServerState,
) -> Result<(), ClusterServerError> {
    if state.config.load_mode == LoadMode::Lazy {
        return Ok(());
    }
    prepare_snapshot_with(state, &state.startup.snapshot, false, preload_route).await?;
    check_startup_snapshot(state)
}

pub(super) async fn preload_route(
    route: PreparedClusterRoute,
    leases: Vec<RouteRequestLease>,
) -> Result<(), ClusterServerError> {
    let endpoint = ensure_model_endpoint(&route.local)
        .await
        .map_err(|error| ClusterServerError::route_unavailable(error.message))?;
    // Dropping an HTTP waiter cannot cancel Python's accepted native work.
    let mut pending = PendingPreload {
        leases,
        completed: false,
    };
    let result = preload_and_record(&route.local, &endpoint).await;
    pending.completed = result.as_ref().err().is_none_or(|error| {
        matches!(
            error.kind,
            ModelRuntimePreloadFailureKind::LoadFailed
                | ModelRuntimePreloadFailureKind::NotAccepted
        )
    });
    result.map_err(|error| ClusterServerError::route_unavailable(error.to_string()))
}

fn check_startup_snapshot(state: &ClusterServerState) -> Result<(), ClusterServerError> {
    if state.definitions.candidate(true)?.is_some() {
        return Err(ClusterServerError::definition_reload_failed(
            "definition changed during eager startup; restart to validate the new routes".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn prepare_with<F, Fut>(
    state: &ClusterServerState,
    preload: F,
) -> Result<(), ClusterServerError>
where
    F: Fn(PreparedClusterRoute, Vec<RouteRequestLease>) -> Fut,
    Fut: Future<Output = Result<(), ClusterServerError>>,
{
    if state.config.load_mode == LoadMode::Lazy {
        return Ok(());
    }
    prepare_snapshot_with(state, &state.startup.snapshot, false, preload).await?;
    check_startup_snapshot(state)
}

pub(super) async fn prepare_snapshot_with<F, Fut>(
    state: &ClusterServerState,
    snapshot: &ClusterDefinitionSnapshot,
    staged: bool,
    preload: F,
) -> Result<(), ClusterServerError>
where
    F: Fn(PreparedClusterRoute, Vec<RouteRequestLease>) -> Fut,
    Fut: Future<Output = Result<(), ClusterServerError>>,
{
    if !matches!(
        snapshot.definition.routes.get(&ClusterRouteKey::Chat),
        Some(ClusterRouteTarget::LocalModel { .. })
    ) {
        return Err(ClusterServerError::route_unavailable(
            "eager cluster startup requires a local routes.chat target".into(),
        ));
    }
    let mut prepared = Vec::new();
    for (route, target) in &snapshot.definition.routes {
        if matches!(target, ClusterRouteTarget::LocalModel { .. }) {
            prepared.push(
                state
                    .prepare_route(*route, &snapshot.definition)
                    .map_err(|error| route_error(&[*route], error))?,
            );
        }
    }
    // Resolve all routes before creating claims or loading any model.
    for group in group_routes(prepared) {
        if !state.routes.is_accepting() {
            return Err(ClusterServerError::route_unavailable(
                "cluster startup stopped during drain".into(),
            ));
        }
        let keys = group.iter().map(|route| route.route).collect::<Vec<_>>();
        let mut leases = Vec::new();
        for route in &group {
            let lease = if staged {
                state
                    .routes
                    .acquire_staged(route.route, &snapshot.hash, route.identity.clone())
            } else {
                state
                    .routes
                    .acquire(route.route, &snapshot.hash, route.identity.clone())
            };
            leases.push(lease.map_err(|error| route_error(&keys, error))?);
        }
        preload(
            group.into_iter().next().expect("nonempty route group"),
            leases,
        )
        .await
        .map_err(|error| route_error(&keys, error))?;
    }
    Ok(())
}

fn group_routes(routes: Vec<PreparedClusterRoute>) -> Vec<Vec<PreparedClusterRoute>> {
    let mut groups = Vec::<Vec<PreparedClusterRoute>>::new();
    let mut indices = BTreeMap::new();
    for route in routes {
        let key = route.identity.physical_key();
        let index = *indices.entry(key).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[index].push(route);
    }
    groups
}

fn route_error(routes: &[ClusterRouteKey], error: ClusterServerError) -> ClusterServerError {
    ClusterServerError::route_unavailable(format!(
        "eager startup route(s) [{}]: {error}",
        routes
            .iter()
            .map(|route| route.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

struct PendingPreload {
    leases: Vec<RouteRequestLease>,
    completed: bool,
}

impl Drop for PendingPreload {
    fn drop(&mut self) {
        if !self.completed {
            for lease in &self.leases {
                lease.preserve_unresolved_preload();
            }
        }
    }
}

#[cfg(test)]
mod tests;
