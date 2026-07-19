use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tentgent_kernel::{
    features::{
        cluster::{
            domain::{
                ClusterDefinition, ClusterRef, ClusterRouteKey, ClusterRouteTarget,
                ClusterStoreLayout, CLUSTER_SCHEMA_VERSION,
            },
            infra::{FileClusterCatalogStore, StdClusterStoreLayoutInitializer},
            ports::{ClusterCatalogStore, ClusterStoreLayoutInitializer},
        },
        model::domain::ModelRef,
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};

use super::*;
use crate::server::cluster::{
    error::ClusterServerError,
    watch::{port::DefinitionRevisionObserver, probes::PlatformDefinitionRevisionProbe},
};

struct SyntheticTicks {
    origin: Instant,
    ticks: VecDeque<Instant>,
}

impl DefinitionWatchTickSource for SyntheticTicks {
    fn origin(&self) -> Instant {
        self.origin
    }

    fn next_tick(&mut self) -> DefinitionWatchTickFuture<'_> {
        let tick = self.ticks.pop_front();
        Box::pin(async move { tick })
    }
}

struct PendingTicks {
    origin: Instant,
}

impl DefinitionWatchTickSource for PendingTicks {
    fn origin(&self) -> Instant {
        self.origin
    }

    fn next_tick(&mut self) -> DefinitionWatchTickFuture<'_> {
        Box::pin(std::future::pending())
    }
}

struct SequenceProbe {
    force_hash_calls: Mutex<Vec<bool>>,
    results: Mutex<VecDeque<Result<String, String>>>,
}

impl DefinitionRevisionProbe for SequenceProbe {
    fn refresh(
        &self,
        _cache: &ClusterDefinitionCache,
        force_hash: bool,
    ) -> Result<String, ClusterServerError> {
        self.force_hash_calls.lock().unwrap().push(force_hash);
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("probe result")
            .map_err(ClusterServerError::definition_reload_failed)
    }
}

#[derive(Default)]
struct RecordingObserver {
    hashes: Mutex<Vec<String>>,
}

impl DefinitionRevisionObserver for RecordingObserver {
    fn reconcile_definition(&self, hash: &str) {
        self.hashes.lock().unwrap().push(hash.to_string());
    }
}

#[tokio::test]
async fn synthetic_ticks_cover_polling_forced_hash_failure_and_recovery() {
    let (cache, home) = cache_fixture("synthetic");
    let origin = Instant::now();
    let probe = Arc::new(SequenceProbe {
        force_hash_calls: Mutex::new(Vec::new()),
        results: Mutex::new(VecDeque::from([
            Ok("hash-a".to_string()),
            Ok("hash-a".to_string()),
            Err("invalid reload".to_string()),
            Ok("hash-b".to_string()),
        ])),
    });
    let observer = Arc::new(RecordingObserver::default());
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    run_definition_watcher_with_dependencies(
        cache,
        cancel_rx,
        DefinitionWatcherDependencies {
            probe: probe.clone(),
            observer: observer.clone(),
            ticks: Box::new(SyntheticTicks {
                origin,
                ticks: VecDeque::from([
                    origin + Duration::from_secs(1),
                    origin + Duration::from_secs(30),
                    origin + Duration::from_secs(31),
                    origin + Duration::from_secs(60),
                ]),
            }),
            schedule: DefinitionWatchSchedule::default(),
        },
    )
    .await;

    assert_eq!(
        *probe.force_hash_calls.lock().unwrap(),
        [false, true, false, true]
    );
    assert_eq!(
        *observer.hashes.lock().unwrap(),
        ["hash-a", "hash-a", "hash-b"]
    );
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn cancellation_stops_a_pending_watcher() {
    let (cache, home) = cache_fixture("cancel");
    let origin = Instant::now();
    let probe = Arc::new(SequenceProbe {
        force_hash_calls: Mutex::new(Vec::new()),
        results: Mutex::new(VecDeque::new()),
    });
    let observer = Arc::new(RecordingObserver::default());
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let watcher = tokio::spawn(run_definition_watcher_with_dependencies(
        cache,
        cancel_rx,
        DefinitionWatcherDependencies {
            probe: probe.clone(),
            observer,
            ticks: Box::new(PendingTicks { origin }),
            schedule: DefinitionWatchSchedule::default(),
        },
    ));
    tokio::task::yield_now().await;
    cancel_tx.send(true).expect("cancel watcher");
    tokio::time::timeout(Duration::from_millis(100), watcher)
        .await
        .expect("watcher shutdown")
        .expect("watcher task");
    assert!(probe.force_hash_calls.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
#[ignore = "manual non-contractual watcher cost measurement"]
fn unchanged_definition_metadata_probe_cost() {
    let (cache, home) = cache_fixture("measurement");
    let probe = PlatformDefinitionRevisionProbe;
    let iterations = 10_000_u32;
    let started = Instant::now();
    for _ in 0..iterations {
        probe
            .refresh(&cache, false)
            .expect("unchanged metadata probe");
    }
    let elapsed = started.elapsed();
    println!(
        "unchanged metadata probes: iterations={iterations} elapsed_ms={} average_ns={}",
        elapsed.as_millis(),
        elapsed.as_nanos() / u128::from(iterations)
    );
    let _ = std::fs::remove_dir_all(home);
}

fn cache_fixture(label: &str) -> (ClusterDefinitionCache, std::path::PathBuf) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let home = std::env::temp_dir().join(format!(
        "tentgent-cluster-watcher-{label}-{}-{nanos}",
        std::process::id()
    ));
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let store = ClusterStoreLayout::from_home_dir(layout.home_dir.clone());
    StdClusterStoreLayoutInitializer
        .ensure_cluster_store_layout(&store)
        .expect("cluster store");
    let cluster_ref = ClusterRef::parse("watcher-test").expect("cluster ref");
    FileClusterCatalogStore
        .save_cluster(
            &store,
            &ClusterDefinition {
                schema_version: CLUSTER_SCHEMA_VERSION,
                cluster_ref: cluster_ref.clone(),
                route_update_policy: Default::default(),
                routes: BTreeMap::from([(
                    ClusterRouteKey::Chat,
                    ClusterRouteTarget::LocalModel {
                        model_ref: ModelRef::parse("a".repeat(64)).expect("model ref"),
                        runtime_profile: None,
                    },
                )]),
            },
        )
        .expect("cluster definition");
    (
        ClusterDefinitionCache::load(&layout, cluster_ref).expect("cache"),
        home,
    )
}
