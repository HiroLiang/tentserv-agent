use std::{
    collections::BTreeMap,
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tentgent_kernel::features::{
    cluster::{
        domain::{ClusterRef, ClusterRouteKey, ClusterRouteTarget, ClusterStoreLayout},
        infra::FileClusterCatalogStore,
        ports::ClusterCatalogStore,
    },
    model::domain::ModelRef,
    runtime::infra::ModelRuntimeCapability,
    runtime_ownership::RuntimeExecutionIdentity,
    server::domain::ServerRuntimeProfileSelection,
};
use tower::ServiceExt;

use super::super::{
    leases::RouteGenerationManager,
    lifecycle::serve_cluster,
    router::cluster_router,
    tests::{definition, state_for_definition, RecordingRouteOwnership},
};
use super::*;

const MODEL: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture(routes: &[ClusterRouteKey]) -> (ClusterServerState, std::path::PathBuf) {
    let mut definition = definition(
        &ClusterRef::parse("startup").unwrap(),
        &ModelRef::parse(MODEL).unwrap(),
        false,
    );
    definition.routes = routes
        .iter()
        .map(|route| {
            (
                *route,
                ClusterRouteTarget::LocalModel {
                    model_ref: ModelRef::parse(MODEL).unwrap(),
                    runtime_profile: None,
                },
            )
        })
        .collect();
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let label = format!("eager-startup-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (mut state, home) = state_for_definition(&label, definition);
    state.config.load_mode = LoadMode::Eager;
    state.startup = ClusterStartupState::new(state.definitions.current().unwrap(), LoadMode::Eager);
    let store = home.join("models/store").join(MODEL);
    let source = store.join("variants/safetensors/source");
    fs::create_dir_all(&source).unwrap();
    fs::write(store.join("manifest.json"), "{}").unwrap();
    fs::write(source.parent().unwrap().join("variant.toml"),
        "format = \"safetensors\"\nstatus = \"imported\"\nimport_method = \"add\"\nrelative_source_path = \"source\"\n").unwrap();
    for file in [
        "config.json",
        "tokenizer.json",
        "model.safetensors",
        "preprocessor_config.json",
    ] {
        fs::write(source.join(file), "{}").unwrap();
    }
    fs::write(store.join("model.toml"), format!(
        "model_ref = \"{MODEL}\"\nshort_ref = \"{}\"\nsource_kind = \"local\"\nprimary_format = \"safetensors\"\ndetected_formats = [\"safetensors\"]\nmodel_capabilities = [\"chat\", \"embedding\", \"rerank\", \"audio-transcription\", \"vision-chat\"]\nmodel_capability_source = \"explicit-user\"\nfile_count = 3\ntotal_bytes = 6\nimported_at = \"2026-09-28T00:00:00Z\"\n", &MODEL[..12])).unwrap();
    (state, home)
}

async fn health(state: &ClusterServerState) -> serde_json::Value {
    let response = cluster_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 10240)
            .await
            .unwrap(),
    )
    .unwrap()
}

fn save_definition(
    state: &ClusterServerState,
    routes: BTreeMap<ClusterRouteKey, ClusterRouteTarget>,
) {
    let mut definition = state.startup.snapshot.definition.clone();
    definition.routes = routes;
    FileClusterCatalogStore
        .save_cluster(
            &ClusterStoreLayout::from_home_dir(state.layout.home_dir.clone()),
            &definition,
        )
        .unwrap();
}

#[tokio::test]
async fn eager_preloads_all_five_local_routes_sequentially_before_readiness() {
    let keys = [
        ClusterRouteKey::Chat,
        ClusterRouteKey::Embedding,
        ClusterRouteKey::Rerank,
        ClusterRouteKey::AudioTranscription,
        ClusterRouteKey::VisionChat,
    ];
    let (state, home) = fixture(&keys);
    let seen = Mutex::new(Vec::new());
    prepare_with(&state, |route, leases| {
        assert!(!state.startup.is_ready());
        assert_eq!(state.routes.active_request_count(), 1);
        assert_eq!(leases.len(), 1);
        seen.lock().unwrap().push(route.route);
        async move {
            tokio::task::yield_now().await;
            drop(leases);
            Ok(())
        }
    })
    .await
    .unwrap();
    assert_eq!(
        seen.into_inner().unwrap(),
        state
            .startup
            .snapshot
            .definition
            .routes
            .keys()
            .copied()
            .collect::<Vec<_>>()
    );
    assert_eq!(state.routes.active_claim_count(), 5);
    assert_eq!(state.routes.active_request_count(), 0);
    assert_eq!(health(&state).await["ready"], false);
    state.startup.mark_ready();
    assert_eq!(health(&state).await["ready"], true);
    state.routes.begin_drain();
    state.routes.finish_drain().await.unwrap();
    assert_eq!(state.routes.active_claim_count(), 0);
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn lazy_does_not_resolve_or_preload_even_unavailable_models() {
    let (mut state, home) = fixture(&[ClusterRouteKey::Chat]);
    state.config.load_mode = LoadMode::Lazy;
    fs::remove_dir_all(home.join("models")).unwrap();
    prepare_with(&state, |_, _| async { panic!("lazy must not preload") })
        .await
        .unwrap();
    assert_eq!(state.routes.active_claim_count(), 0);
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn unavailable_declared_route_fails_before_loading_anything() {
    let (mut state, home) = fixture(&[ClusterRouteKey::Chat, ClusterRouteKey::Embedding]);
    if let ClusterRouteTarget::LocalModel { model_ref, .. } = state
        .startup
        .snapshot
        .definition
        .routes
        .get_mut(&ClusterRouteKey::Embedding)
        .unwrap()
    {
        *model_ref = ModelRef::parse("b".repeat(64)).unwrap();
    }
    let error = prepare_with(&state, |_, _| async { panic!("all routes resolve first") })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("embedding"), "{error}");
    assert_eq!(state.routes.active_claim_count(), 0);
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn provider_optional_route_is_skipped_but_still_not_executable() {
    let (mut state, home) = fixture(&[ClusterRouteKey::Chat]);
    state.startup.snapshot.definition.routes.insert(
        ClusterRouteKey::Embedding,
        ClusterRouteTarget::Provider {
            provider: tentgent_kernel::features::server::domain::CloudProvider::OpenAI,
            provider_model: "embedding-model".into(),
        },
    );
    save_definition(&state, state.startup.snapshot.definition.routes.clone());
    state.startup.snapshot = state.definitions.refresh(true).unwrap();
    prepare_with(&state, |route, _leases| async move {
        assert_eq!(route.route, ClusterRouteKey::Chat);
        Ok(())
    })
    .await
    .unwrap();
    state.startup.mark_ready();
    let error = state
        .resolve_local_state(ClusterRouteKey::Embedding)
        .err()
        .unwrap();
    assert!(error
        .to_string()
        .contains("cluster_route_target_unsupported"));
    state.routes.begin_drain();
    state.routes.finish_drain().await.unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn eager_requires_local_chat_and_partial_failure_cleans_confirmed_claims() {
    let (state, home) = fixture(&[ClusterRouteKey::Embedding]);
    assert!(
        prepare_with(&state, |_, _| async { panic!("missing chat") })
            .await
            .unwrap_err()
            .to_string()
            .contains("routes.chat")
    );
    fs::remove_dir_all(home).unwrap();
    let (state, home) = fixture(&[ClusterRouteKey::Chat, ClusterRouteKey::Embedding]);
    let seen = Mutex::new(0);
    let error = prepare_with(&state, |route, _leases| {
        *seen.lock().unwrap() += 1;
        async move {
            if route.route == ClusterRouteKey::Embedding {
                Err(ClusterServerError::route_unavailable(
                    "injected load failure".into(),
                ))
            } else {
                Ok(())
            }
        }
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("embedding"));
    assert_eq!(*seen.lock().unwrap(), 2);
    assert!(!state.startup.is_ready());
    state.routes.begin_drain();
    state.routes.finish_drain().await.unwrap();
    assert_eq!(state.routes.active_claim_count(), 0);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn deduplication_uses_full_physical_key_not_route_or_model_alone() {
    let (state, home) = fixture(&[ClusterRouteKey::Chat]);
    let mut routes = Vec::new();
    for (capability, profile, version) in [
        (ModelRuntimeCapability::Chat, "p", 1),
        (ModelRuntimeCapability::Chat, "p", 1),
        (ModelRuntimeCapability::Embedding, "p", 1),
        (ModelRuntimeCapability::Chat, "q", 1),
        (ModelRuntimeCapability::Chat, "p", 2),
    ] {
        let mut route = state
            .prepare_route(ClusterRouteKey::Chat, &state.startup.snapshot.definition)
            .unwrap();
        route.identity = RuntimeExecutionIdentity::model_bound(
            MODEL,
            capability,
            Some(&ServerRuntimeProfileSelection::new(profile, version)),
        );
        routes.push(route);
    }
    assert_eq!(
        group_routes(routes)
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [2, 1, 1, 1]
    );
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn starting_health_does_not_refresh_and_inference_is_not_admitted() {
    let (state, home) = fixture(&[ClusterRouteKey::Chat]);
    save_definition(&state, BTreeMap::new());
    assert_eq!(
        health(&state).await["definition_hash"],
        state.startup.snapshot.hash
    );
    for path in [
        "/v1/chat",
        "/v1/embeddings",
        "/v1/rerank",
        "/v1/audio/transcriptions",
        "/v1/vision/chat",
    ] {
        let response = cluster_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
    }
    let error = prepare_with(&state, |_, _leases| async { Ok(()) })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("definition changed"));
    assert!(!state.startup.is_ready());
    state.routes.begin_drain();
    state.routes.finish_drain().await.unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn uncertain_preload_retains_only_its_claim_and_terminal_preload_releases() {
    let (mut state, home) = fixture(&[ClusterRouteKey::Chat, ClusterRouteKey::Embedding]);
    let ownership = Arc::new(RecordingRouteOwnership::default());
    state.routes = RouteGenerationManager::new_with_ownership(
        state.layout.clone(),
        state.config.server_ref.clone(),
        state.config.cluster_ref.clone(),
        ownership.clone(),
    );
    for (route, completed) in [
        (ClusterRouteKey::Chat, true),
        (ClusterRouteKey::Embedding, false),
    ] {
        let prepared = state
            .prepare_route(route, &state.startup.snapshot.definition)
            .unwrap();
        let lease = state
            .routes
            .acquire(route, &state.startup.snapshot.hash, prepared.identity)
            .unwrap();
        drop(PendingPreload {
            leases: vec![lease],
            completed,
        });
    }
    state.routes.reconcile_definition("different");
    state.routes.begin_drain();
    assert!(state
        .routes
        .finish_drain()
        .await
        .unwrap_err()
        .to_string()
        .contains("reconcile"));
    assert_eq!(state.routes.active_claim_count(), 1);
    assert_eq!(
        ownership
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.starts_with("release:"))
            .count(),
        1
    );
    fs::remove_dir_all(home).unwrap();
}

#[tokio::test]
async fn stop_during_startup_drains_current_route_without_loading_next_or_becoming_ready() {
    lifecycle_stop(false).await;
}

#[tokio::test]
async fn stop_drain_timeout_preserves_unfinished_claim() {
    lifecycle_stop(true).await;
}

async fn lifecycle_stop(timeout: bool) {
    let (state, home) = fixture(&[ClusterRouteKey::Chat, ClusterRouteKey::Embedding]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let started = tokio::sync::Notify::new();
    let release = tokio::sync::Notify::new();
    let calls = Mutex::new(0);
    let startup = prepare_with(&state, |_, leases| {
        *calls.lock().unwrap() += 1;
        let started = &started;
        let release = &release;
        async move {
            let mut pending = PendingPreload {
                leases,
                completed: false,
            };
            started.notify_one();
            release.notified().await;
            pending.completed = true;
            Ok(())
        }
    });
    let shutdown = async {
        started.notified().await;
    };
    let release_work = async {
        while state.routes.is_accepting() {
            tokio::task::yield_now().await;
        }
        if !timeout {
            release.notify_one();
        }
    };
    let (result, _) = tokio::join!(
        serve_cluster(
            listener,
            &state,
            startup,
            shutdown,
            Duration::from_millis(100)
        ),
        release_work
    );
    assert_eq!(result.is_err(), timeout);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(!state.startup.is_ready());
    assert_eq!(state.routes.active_request_count(), 0);
    assert_eq!(state.routes.active_claim_count(), usize::from(timeout));
    fs::remove_dir_all(home).unwrap();
}
