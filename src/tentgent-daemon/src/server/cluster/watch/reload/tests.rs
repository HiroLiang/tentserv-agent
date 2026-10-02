use super::super::port::DefinitionWatchTickFuture;
use super::*;
use crate::server::cluster::{
    error::ClusterServerError,
    leases::RouteRequestLease,
    startup::ClusterStartupState,
    tests::{definition, state_for_definition, write_mlx_chat_model_fixture},
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use tentgent_kernel::features::{
    cluster::{
        domain::{ClusterRef, ClusterRouteKey, ClusterStoreLayout},
        infra::FileClusterCatalogStore,
        ports::ClusterCatalogStore,
    },
    model::domain::ModelRef,
    server::options::LoadMode,
};
use tokio::sync::{mpsc, oneshot};

struct Ticks {
    origin: Instant,
    rx: mpsc::UnboundedReceiver<Instant>,
}
impl DefinitionWatchTickSource for Ticks {
    fn origin(&self) -> Instant {
        self.origin
    }
    fn next_tick(&mut self) -> DefinitionWatchTickFuture<'_> {
        Box::pin(self.rx.recv())
    }
}
struct Load {
    model: String,
    complete: oneshot::Sender<bool>,
}
struct ControlledPreparer(mpsc::UnboundedSender<Load>);
struct Pending {
    leases: Vec<RouteRequestLease>,
    completed: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if !self.completed {
            for lease in &self.leases {
                lease.preserve_unresolved_preload();
            }
        }
    }
}
impl CandidatePreparer for ControlledPreparer {
    fn prepare<'a>(
        &'a self,
        state: &'a ClusterServerState,
        snapshot: &'a ClusterDefinitionSnapshot,
    ) -> CandidatePrepareFuture<'a> {
        Box::pin(prepare_snapshot_with(
            state,
            snapshot,
            true,
            move |route, leases| async move {
                let mut pending = Pending {
                    leases,
                    completed: false,
                };
                let (complete, result) = oneshot::channel();
                self.0
                    .send(Load {
                        model: route.local.config.model_ref,
                        complete,
                    })
                    .unwrap();
                let success = result.await.unwrap();
                pending.completed = true;
                if success {
                    Ok(())
                } else {
                    Err(ClusterServerError::route_unavailable(
                        "fixture load failed".into(),
                    ))
                }
            },
        ))
    }
}
struct Harness {
    state: ClusterServerState,
    home: PathBuf,
    ticks: mpsc::UnboundedSender<Instant>,
    loads: mpsc::UnboundedReceiver<Load>,
    cancel: watch::Sender<bool>,
    worker: tokio::task::JoinHandle<()>,
}
impl Harness {
    fn new(label: &str) -> Self {
        Self::with_ownership(label, None)
    }
    fn with_ownership(
        label: &str,
        ownership: Option<std::sync::Arc<crate::server::cluster::tests::RecordingRouteOwnership>>,
    ) -> Self {
        let (mut state, home) = state_for_definition(
            label,
            definition(
                &ClusterRef::parse("reload").unwrap(),
                &ModelRef::parse("a".repeat(64)).unwrap(),
                false,
            ),
        );
        for letter in ["a", "b", "c"] {
            write_mlx_chat_model_fixture(&home, &letter.repeat(64));
        }
        if let Some(ownership) = ownership {
            state.routes = RouteGenerationManager::new_with_ownership(
                state.layout.clone(),
                state.config.server_ref.clone(),
                state.config.cluster_ref.clone(),
                ownership,
            );
        }
        state.config.load_mode = LoadMode::Eager;
        state.startup =
            ClusterStartupState::new(state.definitions.committed().unwrap(), LoadMode::Eager);
        state.startup.mark_ready();
        let (ticks, rx) = mpsc::unbounded_channel();
        let (loads_tx, loads) = mpsc::unbounded_channel();
        let (cancel, cancelled) = watch::channel(false);
        let cloned = state.clone();
        let worker = tokio::spawn(async move {
            run_eager_watcher(
                cloned,
                cancelled,
                Box::new(Ticks {
                    origin: Instant::now(),
                    rx,
                }),
                DefinitionWatchSchedule::default(),
                &ControlledPreparer(loads_tx),
            )
            .await;
        });
        Self {
            state,
            home,
            ticks,
            loads,
            cancel,
            worker,
        }
    }
    fn save(&self, letter: &str) {
        FileClusterCatalogStore
            .save_cluster(
                &ClusterStoreLayout::from_home_dir(self.home.clone()),
                &definition(
                    &self.state.config.cluster_ref,
                    &ModelRef::parse(letter.repeat(64)).unwrap(),
                    false,
                ),
            )
            .unwrap();
    }
    fn tick(&self) {
        self.ticks
            .send(Instant::now() + Duration::from_secs(60))
            .unwrap();
    }
    async fn load(&mut self, letter: &str) -> oneshot::Sender<bool> {
        let load = tokio::time::timeout(Duration::from_secs(2), self.loads.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(load.model, letter.repeat(64));
        load.complete
    }
    fn routed(&self) -> String {
        self.state
            .resolve_local_state(ClusterRouteKey::Chat)
            .unwrap()
            .local
            .config
            .model_ref
    }
    async fn status(&self, status: &str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.state.reload_status.snapshot().status != status {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn close(self) {
        self.cancel.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), self.worker)
            .await
            .unwrap()
            .unwrap();
        self.state.routes.begin_drain();
        self.state.routes.finish_drain().await.unwrap();
        assert_eq!(self.state.routes.active_claim_count(), 0);
        fs::remove_dir_all(self.home).unwrap();
    }
}

#[tokio::test]
async fn rollback_retries_after_old_stream_drains_without_reapplying_definition() {
    let mut h = Harness::new("reload-rollback-drain");
    let old_stream = h.state.resolve_local_state(ClusterRouteKey::Chat).unwrap();
    h.save("b");
    h.tick();
    h.load("b").await.send(true).unwrap();
    h.status("idle").await;
    assert_eq!(h.routed(), "b".repeat(64));

    h.save("a");
    h.tick();
    h.status("waiting").await;
    assert!(
        h.loads.try_recv().is_err(),
        "retiring A cannot start a loader"
    );
    assert_eq!(h.routed(), "b".repeat(64));
    assert_eq!(h.state.routes.active_claim_count(), 2);
    drop(old_stream);
    h.tick();
    h.load("a").await.send(true).unwrap();
    h.status("idle").await;
    assert_eq!(h.routed(), "a".repeat(64));
    assert_eq!(h.state.routes.active_claim_count(), 1);
    h.close().await;
}

#[tokio::test]
async fn transient_claim_acquisition_retries_same_candidate_on_next_tick() {
    let ownership =
        std::sync::Arc::new(crate::server::cluster::tests::RecordingRouteOwnership::default());
    let mut h = Harness::with_ownership("reload-busy-acquire", Some(ownership.clone()));
    h.routed();
    h.save("b");
    ownership.busy_on_next_acquire();
    h.tick();
    h.status("waiting").await;
    assert!(
        h.loads.try_recv().is_err(),
        "busy claims cannot start a loader"
    );
    assert_eq!(h.routed(), "a".repeat(64));
    h.tick();
    h.load("b").await.send(true).unwrap();
    h.status("idle").await;
    assert_eq!(h.routed(), "b".repeat(64));
    h.close().await;
}

#[tokio::test]
async fn idle_ticks_retry_busy_retired_claim_release_without_a_new_revision() {
    let ownership =
        std::sync::Arc::new(crate::server::cluster::tests::RecordingRouteOwnership::default());
    let mut h = Harness::with_ownership("reload-busy-release", Some(ownership.clone()));
    h.routed();
    h.save("b");
    h.tick();
    let b = h.load("b").await;
    ownership.busy_on_next_release();
    b.send(true).unwrap();
    h.status("idle").await;
    assert_eq!(h.state.routes.active_claim_count(), 2);
    h.tick();
    tokio::time::timeout(Duration::from_secs(2), async {
        while h.state.routes.active_claim_count() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.routed(), "b".repeat(64));
    h.close().await;
}

#[tokio::test]
async fn old_traffic_does_not_retire_staged_candidate_and_old_stream_drains_after_promotion() {
    let mut h = Harness::new("reload-old-traffic");
    let old_stream = h.state.resolve_local_state(ClusterRouteKey::Chat).unwrap();
    h.save("b");
    h.tick();
    let done = h.load("b").await;
    assert_eq!(h.routed(), "a".repeat(64));
    assert_eq!(h.state.routes.active_claim_count(), 2);
    // Also defend against a legacy observer accidentally reconciling old traffic.
    h.state
        .routes
        .reconcile_definition(&h.state.startup.snapshot.hash);
    assert_eq!(h.state.routes.active_claim_count(), 2);
    done.send(true).unwrap();
    h.status("idle").await;
    assert_eq!(h.routed(), "b".repeat(64));
    assert_eq!(old_stream.local.config.model_ref, "a".repeat(64));
    assert_eq!(h.state.routes.active_claim_count(), 2);
    drop(old_stream);
    assert_eq!(h.state.routes.active_claim_count(), 1);
    h.close().await;
}

#[tokio::test]
async fn failed_candidate_keeps_old_routes_and_is_not_retried_on_every_tick() {
    let mut h = Harness::new("reload-failure");
    assert_eq!(h.routed(), "a".repeat(64));
    h.save("b");
    h.tick();
    h.load("b").await.send(false).unwrap();
    h.status("failed").await;
    assert!(h
        .state
        .reload_status
        .snapshot()
        .diagnostic
        .unwrap()
        .contains("fixture load failed"));
    assert_eq!(h.routed(), "a".repeat(64));
    assert_eq!(h.state.routes.active_claim_count(), 1);
    for _ in 0..3 {
        h.tick();
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), h.loads.recv())
            .await
            .is_err()
    );
    h.save("c");
    h.tick();
    h.load("c").await.send(true).unwrap();
    h.status("idle").await;
    assert_eq!(h.routed(), "c".repeat(64));
    h.close().await;
}

#[tokio::test]
async fn superseded_b_never_publishes_and_c_starts_only_after_b_finishes() {
    let mut h = Harness::new("reload-superseded");
    h.routed();
    h.save("b");
    h.tick();
    let b = h.load("b").await;
    h.save("c");
    assert_eq!(h.routed(), "a".repeat(64));
    b.send(true).unwrap();
    h.status("superseded").await;
    assert_eq!(h.routed(), "a".repeat(64));
    assert_eq!(h.state.routes.active_claim_count(), 1);
    h.tick();
    h.load("c").await.send(true).unwrap();
    h.status("idle").await;
    assert_eq!(h.routed(), "c".repeat(64));
    h.close().await;
}

#[tokio::test]
async fn malformed_candidate_does_not_break_committed_health_or_traffic() {
    let h = Harness::new("reload-malformed");
    let store = ClusterStoreLayout::from_home_dir(h.home.clone());
    fs::write(store.cluster_definition_path("reload"), "invalid = [").unwrap();
    h.tick();
    h.status("failed").await;
    assert_eq!(h.routed(), "a".repeat(64));
    assert!(h.state.startup.is_ready());
    assert_eq!(
        h.state.definition_snapshot().unwrap().hash,
        h.state.startup.snapshot.hash
    );
    h.close().await;
}

#[tokio::test]
async fn paused_snapshot_selection_retries_after_promotion_without_recreating_old_claim() {
    let mut h = Harness::new("reload-admission-race");
    let selected = h.state.definitions.committed().unwrap();
    let prepared = h
        .state
        .prepare_route(ClusterRouteKey::Chat, &selected.definition);
    h.save("b");
    h.tick();
    h.load("b").await.send(true).unwrap();
    h.status("idle").await;
    assert!(h
        .state
        .admit_prepared(&selected, prepared)
        .unwrap()
        .is_none());
    assert_eq!(h.routed(), "b".repeat(64));
    assert_eq!(h.state.routes.active_claim_count(), 1);
    h.close().await;
}

#[tokio::test]
async fn stopping_during_preload_observes_completion_without_promotion() {
    let mut h = Harness::new("reload-stop");
    h.routed();
    h.save("b");
    h.tick();
    let b = h.load("b").await;
    h.cancel.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while h.state.routes.is_accepting() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!h.worker.is_finished());
    assert!(h.state.resolve_local_state(ClusterRouteKey::Chat).is_err());
    b.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), &mut h.worker)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        h.state.definitions.committed().unwrap().hash,
        h.state.startup.snapshot.hash
    );
    h.state.routes.finish_drain().await.unwrap();
    assert_eq!(h.state.routes.active_claim_count(), 0);
    fs::remove_dir_all(h.home).unwrap();
}

#[tokio::test]
async fn timed_out_observer_preserves_accepted_candidate_claim() {
    let mut h = Harness::new("reload-abort");
    h.save("b");
    h.tick();
    let _b = h.load("b").await;
    h.state.routes.begin_drain();
    h.state.routes.preserve_on_timeout();
    h.worker.abort();
    assert!(h.worker.await.unwrap_err().is_cancelled());
    assert_eq!(h.state.routes.active_request_count(), 0);
    assert_eq!(h.state.routes.active_claim_count(), 1);
    assert_eq!(
        h.state.definitions.committed().unwrap().hash,
        h.state.startup.snapshot.hash
    );
    fs::remove_dir_all(h.home).unwrap();
}
