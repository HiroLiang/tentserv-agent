use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    future::poll_fn,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use axum::{
    body::{to_bytes, Body, Bytes, HttpBody},
    extract::State,
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use http_body::Frame;
use serde_json::Value;
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
        runtime::{
            domain::{PythonRuntimeLayout, PythonRuntimeSource},
            infra::{ModelRuntimeDaemonLaunchPolicy, ModelRuntimeDaemonSupervisor},
        },
        runtime_ownership::{
            FileRuntimeOwnershipStore, RouteClaimAcquireRequest, RouteClaimOwnershipUseCase,
            RouteClaimTransition, RuntimeExecutionIdentity, RuntimeOwnershipLayout,
            RuntimeOwnershipStore,
        },
    },
    foundation::{
        error::KernelResult,
        layout::{
            LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
        },
    },
};
use tower::ServiceExt;

use super::{
    cache::ClusterDefinitionCache,
    handlers::attach_lease,
    leases::{RouteGenerationManager, RouteRequestLease},
    router::cluster_router,
    state::{ClusterServerRuntimeConfig, ClusterServerState},
};

struct FrameSequence {
    frames: VecDeque<Result<Frame<Bytes>, std::io::Error>>,
}

#[derive(Clone)]
struct DelayedStreamState {
    manager: RouteGenerationManager,
    release: tokio::sync::watch::Receiver<bool>,
}

#[derive(Default)]
struct RecordingRouteOwnership {
    events: Mutex<Vec<String>>,
    release_busy_remaining: Mutex<usize>,
}

impl RecordingRouteOwnership {
    fn busy_on_next_release(&self) {
        *self.release_busy_remaining.lock().unwrap() = 1;
    }
}

impl RouteClaimOwnershipUseCase for RecordingRouteOwnership {
    fn acquire_route_claim(
        &self,
        _layout: &tentgent_kernel::foundation::layout::RuntimeLayout,
        request: RouteClaimAcquireRequest,
    ) -> KernelResult<RouteClaimTransition> {
        self.events
            .lock()
            .unwrap()
            .push(format!("acquire:{}", request.claim.owner_id));
        Ok(RouteClaimTransition::Acquired(request.claim))
    }

    fn retire_route_claim(
        &self,
        _layout: &tentgent_kernel::foundation::layout::RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<tentgent_kernel::features::resource_coordination::ResourceBusy>> {
        self.events
            .lock()
            .unwrap()
            .push(format!("retire:{owner_id}"));
        Ok(None)
    }

    fn release_route_claim(
        &self,
        _layout: &tentgent_kernel::foundation::layout::RuntimeLayout,
        owner_id: &str,
    ) -> KernelResult<Option<tentgent_kernel::features::resource_coordination::ResourceBusy>> {
        self.events
            .lock()
            .unwrap()
            .push(format!("release:{owner_id}"));
        let mut remaining = self.release_busy_remaining.lock().unwrap();
        if *remaining > 0 {
            *remaining -= 1;
            return Ok(Some(
                tentgent_kernel::features::resource_coordination::ResourceBusy {
                    code: tentgent_kernel::features::resource_coordination::ResourceCoordinationCode::ResourceBusy,
                    operation_id: "busy-release".to_string(),
                    key: tentgent_kernel::features::resource_coordination::ResourceKey::new(
                        tentgent_kernel::features::resource_coordination::ResourceKind::RouteClaim,
                        owner_id,
                    ),
                    attempts: 1,
                    waited_millis: 1,
                    holders: Vec::new(),
                    retry_after_millis: 1,
                    description: "route claim is temporarily busy".to_string(),
                },
            ));
        }
        Ok(None)
    }
}

#[tokio::test]
async fn route_manager_uses_injected_claim_ownership_without_file_store() {
    let home = unique_home("injected-route-ownership");
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let ownership = Arc::new(RecordingRouteOwnership::default());
    let manager = RouteGenerationManager::new_with_ownership(
        layout,
        "server-ref".to_string(),
        ClusterRef::parse("local-assistant").unwrap(),
        ownership.clone(),
    );
    let lease = manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect("route lease");
    manager.begin_drain();
    drop(lease);
    tokio::time::timeout(Duration::from_millis(100), manager.finish_drain())
        .await
        .expect("drain")
        .expect("claim release");

    let events = ownership.events.lock().unwrap();
    assert_eq!(events.len(), 3);
    assert!(events[0].starts_with("acquire:"));
    assert!(events[1].starts_with("retire:"));
    assert!(events[2].starts_with("release:"));
    assert!(!home.join("runtime/ownership").exists());
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn busy_claim_release_stays_tracked_until_drain_retry_succeeds() {
    let home = unique_home("busy-route-release");
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let ownership = Arc::new(RecordingRouteOwnership::default());
    let manager = RouteGenerationManager::new_with_ownership(
        layout,
        "server-ref".to_string(),
        ClusterRef::parse("local-assistant").unwrap(),
        ownership.clone(),
    );
    let lease = manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect("route lease");
    manager.begin_drain();
    ownership.busy_on_next_release();

    drop(lease);
    assert_eq!(manager.active_claim_count(), 1);

    tokio::time::timeout(Duration::from_millis(100), manager.finish_drain())
        .await
        .expect("drain retry")
        .expect("claim release");
    assert_eq!(manager.active_claim_count(), 0);
    let _ = std::fs::remove_dir_all(home);
}

impl FrameSequence {
    fn new(frames: impl IntoIterator<Item = Result<Frame<Bytes>, std::io::Error>>) -> Self {
        Self {
            frames: frames.into_iter().collect(),
        }
    }
}

impl HttpBody for FrameSequence {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(self.frames.pop_front())
    }
}

#[tokio::test]
async fn leased_body_releases_request_at_eof_and_preserves_trailers() {
    let (manager, lease, home) = route_lease_fixture("leased-body-eof");
    let mut trailers = HeaderMap::new();
    trailers.insert("x-tentgent-test", HeaderValue::from_static("complete"));
    let source = FrameSequence::new([
        Ok(Frame::data(Bytes::from_static(b"hello"))),
        Ok(Frame::trailers(trailers)),
    ]);
    let mut source_response = Response::new(Body::new(source));
    source_response
        .headers_mut()
        .insert("x-upstream", HeaderValue::from_static("preserved"));
    let response = attach_lease(source_response, lease);
    assert_eq!(response.headers()["x-upstream"], "preserved");
    assert_eq!(manager.active_request_count(), 1);

    let mut body = response.into_body();
    let data = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
        .await
        .expect("data frame")
        .expect("data result");
    assert_eq!(
        data.into_data().expect("data"),
        Bytes::from_static(b"hello")
    );
    let trailers = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
        .await
        .expect("trailer frame")
        .expect("trailer result")
        .into_trailers()
        .expect("trailers");
    assert_eq!(trailers["x-tentgent-test"], "complete");
    assert!(poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
        .await
        .is_none());
    assert_eq!(manager.active_request_count(), 0);
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn drain_rejects_new_admission_and_releases_claim_after_active_request_finishes() {
    let (manager, lease, home) = route_lease_fixture("successful-drain");
    manager.begin_drain();
    let error = manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect_err("draining server must reject new admission");
    assert!(error.to_string().contains("draining"));

    drop(lease);
    tokio::time::timeout(Duration::from_millis(250), manager.finish_drain())
        .await
        .expect("drain completion")
        .expect("claim release");
    assert_eq!(manager.active_claim_count(), 0);
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn graceful_listener_drain_keeps_lease_after_headers_until_stream_eof() {
    let home = unique_home("listener-drain");
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let manager = RouteGenerationManager::new(
        layout,
        "server-ref".to_string(),
        ClusterRef::parse("local-assistant").unwrap(),
    );
    let (release_tx, release_rx) = tokio::sync::watch::channel(false);
    let router = Router::new()
        .route("/stream", get(delayed_stream))
        .with_state(DelayedStreamState {
            manager: manager.clone(),
            release: release_rx,
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().unwrap();
    let (graceful_tx, graceful_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = graceful_rx.await;
            })
            .await
    });

    let response = reqwest::get(format!("http://{address}/stream"))
        .await
        .expect("stream response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(manager.active_request_count(), 1);

    manager.begin_drain();
    let _ = graceful_tx.send(());
    assert!(manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .is_err());
    let body = tokio::spawn(response.bytes());
    release_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        let (server_result, drain_result) = tokio::join!(server, manager.finish_drain());
        server_result.unwrap().unwrap();
        drain_result.unwrap();
    })
    .await
    .expect("graceful stream drain");
    assert_eq!(
        body.await.unwrap().unwrap(),
        Bytes::from_static(b"complete")
    );
    assert_eq!(manager.active_claim_count(), 0);
    let _ = std::fs::remove_dir_all(home);
}

async fn delayed_stream(State(mut state): State<DelayedStreamState>) -> Response {
    let lease = state
        .manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect("stream route lease");
    let stream = futures_util::stream::once(async move {
        while !*state.release.borrow() {
            state.release.changed().await.expect("release signal");
        }
        Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(b"complete"))
    });
    attach_lease(Response::new(Body::from_stream(stream)), lease)
}

#[tokio::test]
async fn leased_body_releases_request_on_error_and_drop() {
    let (manager, lease, home) = route_lease_fixture("leased-body-error");
    let source = FrameSequence::new([Err(std::io::Error::other("stream failed"))]);
    let response = attach_lease(Response::new(Body::new(source)), lease);
    let mut body = response.into_body();
    assert!(poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
        .await
        .expect("error frame")
        .is_err());
    assert_eq!(manager.active_request_count(), 0);

    let lease = manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect("second route lease");
    let response = attach_lease(Response::new(Body::from("pending")), lease);
    assert_eq!(manager.active_request_count(), 1);
    drop(response);
    assert_eq!(manager.active_request_count(), 0);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn definition_cache_reloads_when_stored_definition_changes() {
    let home = unique_home("reload");
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home),
            data_root_dir: None,
        })
        .expect("layout");
    let store = ClusterStoreLayout::from_home_dir(layout.home_dir.clone());
    StdClusterStoreLayoutInitializer
        .ensure_cluster_store_layout(&store)
        .expect("cluster layout");
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    let model_ref = ModelRef::parse("a".repeat(64)).expect("model ref");
    FileClusterCatalogStore
        .save_cluster(&store, &definition(&cluster_ref, &model_ref, false))
        .expect("save first definition");
    let cache = ClusterDefinitionCache::load(&layout, cluster_ref.clone()).expect("cache");
    let first = cache.current().expect("first snapshot");

    FileClusterCatalogStore
        .save_cluster(&store, &definition(&cluster_ref, &model_ref, true))
        .expect("save changed definition");
    let second = cache.current().expect("second snapshot");

    assert_ne!(first.hash, second.hash);
    assert!(second
        .definition
        .routes
        .contains_key(&ClusterRouteKey::Embedding));
}

#[test]
fn definition_cache_does_not_fall_back_after_invalid_external_edit() {
    let home = unique_home("invalid-reload");
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home),
            data_root_dir: None,
        })
        .expect("layout");
    let store = ClusterStoreLayout::from_home_dir(layout.home_dir.clone());
    StdClusterStoreLayoutInitializer
        .ensure_cluster_store_layout(&store)
        .expect("cluster layout");
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    let model_ref = ModelRef::parse("b".repeat(64)).expect("model ref");
    FileClusterCatalogStore
        .save_cluster(&store, &definition(&cluster_ref, &model_ref, false))
        .expect("save definition");
    let cache = ClusterDefinitionCache::load(&layout, cluster_ref.clone()).expect("cache");
    std::fs::write(
        store.cluster_definition_path(cluster_ref.as_str()),
        "schema_version = 1\ncluster_ref = [",
    )
    .expect("corrupt external edit");

    let error = cache.current().expect_err("invalid reload must fail");
    assert!(format!("{error:?}").contains("cluster_definition_reload_failed"));
}

#[tokio::test]
async fn route_claim_lives_for_generation_and_releases_after_reload_drain() {
    let home = unique_home("claim-drain");
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let manager = super::leases::RouteGenerationManager::new(
        layout.clone(),
        "server-ref".to_string(),
        ClusterRef::parse("local-assistant").unwrap(),
    );
    let lease = manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect("route lease");
    let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
    assert_eq!(
        FileRuntimeOwnershipStore
            .list_claims(&ownership)
            .unwrap()
            .0
            .len(),
        1
    );
    drop(lease);
    assert_eq!(
        FileRuntimeOwnershipStore
            .list_claims(&ownership)
            .unwrap()
            .0
            .len(),
        1,
        "steady-state request completion must not release the generation claim"
    );
    manager.reconcile_definition("definition-b");
    assert_eq!(
        FileRuntimeOwnershipStore
            .list_claims(&ownership)
            .unwrap()
            .0
            .len(),
        0
    );
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn stop_timeout_preserves_unresolved_claim_for_reconciliation() {
    let home = unique_home("claim-timeout");
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let manager = super::leases::RouteGenerationManager::new(
        layout.clone(),
        "server-ref".to_string(),
        ClusterRef::parse("local-assistant").unwrap(),
    );
    let lease = manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect("route lease");
    manager.begin_drain();
    assert!(
        tokio::time::timeout(Duration::from_millis(5), manager.finish_drain())
            .await
            .is_err()
    );
    manager.preserve_on_timeout();
    drop(lease);
    let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
    let claims = FileRuntimeOwnershipStore.list_claims(&ownership).unwrap().0;
    assert_eq!(claims.len(), 1);
    assert_eq!(
        claims[0].state,
        tentgent_kernel::features::runtime_ownership::RouteClaimState::Retiring
    );
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn cluster_router_maps_supported_endpoint_families_without_cross_route_fallback() {
    let (state, home) = provider_chat_state("route-map");
    let router = cluster_router(state);
    for (path, body) in [
        (
            "/v1/chat/completions",
            r#"{"messages":[{"role":"user","content":"hi"}]}"#,
        ),
        (
            "/v1/messages",
            r#"{"model":"caller","messages":[{"role":"user","content":"hi"}],"max_tokens":8}"#,
        ),
        (
            "/v1beta/models/caller:generateContent",
            r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
        ),
        ("/v1/chat", r#"{"messages":[]}"#),
        ("/v1/chat/stream", r#"{"messages":[]}"#),
    ] {
        let response = router
            .clone()
            .oneshot(json_request(path, body))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let value = response_json(response).await;
        assert_eq!(value["error"], "cluster_route_target_unsupported", "{path}");
    }

    for path in [
        "/v1/embeddings",
        "/v1/rerank",
        "/v1/audio/transcriptions",
        "/v1/vision/chat",
    ] {
        let response = router
            .clone()
            .oneshot(json_request(path, "{}"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let value = response_json(response).await;
        assert_eq!(value["error"], "cluster_route_missing", "{path}");
    }

    let response = router
        .oneshot(json_request("/v1/images/generations", "{}"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let value = response_json(response).await;
    assert_eq!(value["error"], "cluster_route_unsupported");
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn cluster_router_keeps_the_bound_route_when_request_model_differs() {
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    let missing_model_ref = ModelRef::parse("c".repeat(64)).expect("model ref");
    let definition = definition(&cluster_ref, &missing_model_ref, false);
    let (state, home) = state_for_definition("bound-model", definition);

    let router = cluster_router(state);
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("health response");
    assert_eq!(response.status(), StatusCode::OK);
    let value = response_json(response).await;
    assert_eq!(
        value["runtime_home"].as_str(),
        Some(home.display().to_string().as_str())
    );

    let response = router
        .oneshot(json_request(
            "/v1/chat/completions",
            r#"{"model":"caller-selected-model","messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let value = response_json(response).await;
    assert_eq!(value["error"], "cluster_route_unavailable");
    assert!(value["message"]
        .as_str()
        .expect("error message")
        .contains(missing_model_ref.as_str()));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn cluster_chat_route_resolves_managed_adapter_through_native_boundary() {
    let cluster_ref = ClusterRef::parse("adapter-cluster").expect("cluster ref");
    let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    let adapter_ref = "e".repeat(64);
    let (state, home) = state_for_definition(
        "managed-adapter",
        definition(&cluster_ref, &model_ref, false),
    );
    write_mlx_chat_model_fixture(&home, model_ref.as_str());
    write_mlx_chat_adapter_fixture(&home, &adapter_ref, model_ref.as_str());

    let resolved = state
        .resolve_local_state(ClusterRouteKey::Chat)
        .expect("resolve cluster chat route");
    let payload = crate::server::local::managed_adapter::resolve_managed_adapter(
        &resolved.local,
        &adapter_ref[..12],
    )
    .expect("resolve managed adapter through native local boundary");

    assert_eq!(payload.adapter_ref, adapter_ref);
    assert_eq!(payload.short_ref, &adapter_ref[..12]);
    assert_eq!(payload.adapter_format, "mlx");
    assert_eq!(payload.adapter_type, "lora");
    assert_eq!(
        fs::canonicalize(&payload.source_path).expect("canonical payload source"),
        fs::canonicalize(
            home.join("adapters/store")
                .join(&adapter_ref)
                .join("source")
        )
        .expect("canonical expected source")
    );
    drop(resolved);
    let _ = fs::remove_dir_all(home);
}

fn definition(
    cluster_ref: &ClusterRef,
    model_ref: &ModelRef,
    include_embedding: bool,
) -> ClusterDefinition {
    let mut routes = BTreeMap::from([(
        ClusterRouteKey::Chat,
        ClusterRouteTarget::LocalModel {
            model_ref: model_ref.clone(),
            runtime_profile: None,
        },
    )]);
    if include_embedding {
        routes.insert(
            ClusterRouteKey::Embedding,
            ClusterRouteTarget::LocalModel {
                model_ref: model_ref.clone(),
                runtime_profile: None,
            },
        );
    }
    ClusterDefinition {
        schema_version: CLUSTER_SCHEMA_VERSION,
        cluster_ref: cluster_ref.clone(),
        route_update_policy: Default::default(),
        routes,
    }
}

fn unique_home(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-cluster-server-{label}-{}-{nanos}",
        std::process::id()
    ))
}

fn write_mlx_chat_model_fixture(home: &std::path::Path, model_ref: &str) {
    let store_dir = home.join("models/store").join(model_ref);
    let source_dir = store_dir.join("variants/mlx/source");
    fs::create_dir_all(&source_dir).expect("model source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("model manifest");
    fs::write(
        store_dir.join("variants/mlx/variant.toml"),
        "format = \"mlx\"\nstatus = \"imported\"\nimport_method = \"add\"\nrelative_source_path = \"source\"\n",
    )
    .expect("model variant");
    fs::write(source_dir.join("config.json"), "{}").expect("model config");
    fs::write(source_dir.join("tokenizer.json"), "{}").expect("model tokenizer");
    fs::write(
        store_dir.join("model.toml"),
        format!(
            "model_ref = \"{model_ref}\"\nshort_ref = \"{}\"\nsource_kind = \"local\"\nsource_path = \"{}\"\nprimary_format = \"mlx\"\ndetected_formats = [\"mlx\"]\nmodel_capabilities = [\"chat\"]\nmodel_capability_source = \"explicit-user\"\nfile_count = 2\ntotal_bytes = 4\nimported_at = \"2026-07-21T00:00:00Z\"\n",
            &model_ref[..12],
            source_dir.display()
        ),
    )
    .expect("model metadata");
}

fn write_mlx_chat_adapter_fixture(home: &std::path::Path, adapter_ref: &str, model_ref: &str) {
    let store_dir = home.join("adapters/store").join(adapter_ref);
    let source_dir = store_dir.join("source");
    fs::create_dir_all(&source_dir).expect("adapter source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("adapter manifest");
    fs::write(source_dir.join("adapters.safetensors"), "adapter").expect("adapter weights");
    fs::write(
        store_dir.join("adapter.toml"),
        format!(
            "adapter_ref = \"{adapter_ref}\"\nshort_ref = \"{}\"\nadapter_format = \"mlx\"\nadapter_type = \"lora\"\ntarget_capability = \"chat\"\nbase_model_ref = \"{model_ref}\"\nbackend_support = [\"mlx\"]\nsource_kind = \"local\"\nsource_path = \"{}\"\nfile_count = 1\ntotal_bytes = 7\nimported_at = \"2026-07-21T00:00:00Z\"\n",
            &adapter_ref[..12],
            source_dir.display()
        ),
    )
    .expect("adapter metadata");
}

fn route_lease_fixture(
    label: &str,
) -> (
    RouteGenerationManager,
    RouteRequestLease,
    std::path::PathBuf,
) {
    let home = unique_home(label);
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let manager = RouteGenerationManager::new(
        layout,
        "server-ref".to_string(),
        ClusterRef::parse("local-assistant").unwrap(),
    );
    let lease = manager
        .acquire(
            ClusterRouteKey::Chat,
            "definition-a",
            RuntimeExecutionIdentity::model_bound(
                "model-a",
                tentgent_kernel::features::runtime::infra::ModelRuntimeCapability::Chat,
                None,
            ),
        )
        .expect("route lease");
    (manager, lease, home)
}

fn provider_chat_state(label: &str) -> (ClusterServerState, std::path::PathBuf) {
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    state_for_definition(
        label,
        ClusterDefinition {
            schema_version: CLUSTER_SCHEMA_VERSION,
            cluster_ref,
            route_update_policy: Default::default(),
            routes: BTreeMap::from([(
                ClusterRouteKey::Chat,
                ClusterRouteTarget::Provider {
                    provider: tentgent_kernel::features::server::domain::CloudProvider::OpenAI,
                    provider_model: "gpt-test".to_string(),
                },
            )]),
        },
    )
}

fn state_for_definition(
    label: &str,
    definition: ClusterDefinition,
) -> (ClusterServerState, std::path::PathBuf) {
    let home = unique_home(label);
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
        .expect("cluster layout");
    let cluster_ref = definition.cluster_ref.clone();
    FileClusterCatalogStore
        .save_cluster(&store, &definition)
        .expect("save cluster");
    let definitions = ClusterDefinitionCache::load(&layout, cluster_ref.clone()).expect("cache");
    let routes = super::leases::RouteGenerationManager::new(
        layout.clone(),
        "server-ref".to_string(),
        cluster_ref.clone(),
    );
    let state = ClusterServerState {
        config: ClusterServerRuntimeConfig {
            server_ref: "server-ref".to_string(),
            cluster_ref,
            host: "127.0.0.1".to_string(),
            port: 0,
            runtime_home: Some(layout.home_dir.clone()),
            idle_seconds: None,
            allow_unverified: true,
        },
        runtime: PythonRuntimeLayout {
            project_dir: layout.runtime_dir.join("project"),
            env_dir: layout.python_env_dir.clone(),
            source: PythonRuntimeSource::DevelopmentSource,
        },
        layout,
        executable_resolver:
            tentgent_kernel::features::runtime::infra::StdRuntimeExecutableResolver,
        supervisor: ModelRuntimeDaemonSupervisor::new(),
        client: reqwest::Client::new(),
        launch_policy: ModelRuntimeDaemonLaunchPolicy::default(),
        definitions,
        routes,
    };
    (state, home)
}

fn json_request(path: &str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("request")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    serde_json::from_slice(&body).expect("json response")
}
