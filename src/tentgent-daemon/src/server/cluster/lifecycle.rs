use std::{
    future::{Future, IntoFuture},
    time::Duration,
};

use super::{error::ClusterServerError, router::cluster_router, state::ClusterServerState, watch};

pub(super) async fn serve_cluster(
    listener: tokio::net::TcpListener,
    state: &ClusterServerState,
    startup: impl Future<Output = Result<(), ClusterServerError>>,
    shutdown_signal: impl Future<Output = ()>,
    drain_timeout: Duration,
) -> Result<(), ClusterServerError> {
    let (watch_cancel_tx, watch_cancel_rx) = tokio::sync::watch::channel(false);
    let mut watcher = None;
    let (graceful_tx, graceful_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, cluster_router(state.clone()))
        .with_graceful_shutdown(async move {
            let _ = graceful_rx.await;
        })
        .into_future();
    tokio::pin!(server, startup, shutdown_signal);
    let mut startup_finished = false;
    let mut server_finished = false;
    let outcome = loop {
        tokio::select! {
            biased;
            _ = &mut shutdown_signal => break Ok(()),
            result = &mut server => {
                server_finished = true;
                break result.map_err(|error| ClusterServerError::route_unavailable(error.to_string()));
            }
            result = &mut startup, if !startup_finished => {
                startup_finished = true;
                if let Err(error) = result { break Err(error); }
                state.startup.mark_ready();
                watcher = Some(tokio::spawn(watch::run_definition_watcher(
                    state.clone(), watch_cancel_rx.clone(),
                )));
            }
        }
    };
    let _ = watch_cancel_tx.send(true);
    state.begin_drain();
    let _ = graceful_tx.send(());
    let mut watcher_joined = false;
    let cleanup = async {
        // Observe accepted Python work during drain, but never publish readiness
        // or start the next route after a stop request.
        if !startup_finished {
            let _ = (&mut startup).await;
        }
        let server_done = async {
            if server_finished {
                Ok(())
            } else {
                (&mut server)
                    .await
                    .map_err(|error| ClusterServerError::route_unavailable(error.to_string()))
            }
        };
        let watcher_done = async {
            if let Some(watcher) = &mut watcher {
                let _ = watcher.await;
            }
            watcher_joined = true;
        };
        let (server_result, drain_result, _) =
            tokio::join!(server_done, state.routes.finish_drain(), watcher_done);
        drain_result.and(server_result)
    };
    let cleanup_result = match tokio::time::timeout(drain_timeout, cleanup).await {
        Ok(result) => result,
        Err(_) => {
            // Set this before dropping startup/response futures and their leases.
            state.routes.preserve_on_timeout();
            Err(ClusterServerError::route_unavailable(
                "cluster drain timed out; retained route claims protect unfinished work. Inspect runtime ownership and reconcile after work finishes".into(),
            ))
        }
    };
    if !watcher_joined {
        if let Some(watcher) = watcher {
            watcher.abort();
            let _ = watcher.await;
        }
    }
    match (outcome, cleanup_result) {
        (Err(startup_error), Err(cleanup_error)) => Err(ClusterServerError::route_unavailable(
            format!("{startup_error}; cleanup: {cleanup_error}"),
        )),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}
