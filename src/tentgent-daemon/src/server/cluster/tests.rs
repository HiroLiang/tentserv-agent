use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
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
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};
use tower::ServiceExt;

use super::{
    cache::ClusterDefinitionCache,
    cluster_router,
    state::{ClusterServerRuntimeConfig, ClusterServerState},
};

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

fn provider_chat_state(label: &str) -> (ClusterServerState, std::path::PathBuf) {
    let cluster_ref = ClusterRef::parse("local-assistant").expect("cluster ref");
    state_for_definition(
        label,
        ClusterDefinition {
            schema_version: CLUSTER_SCHEMA_VERSION,
            cluster_ref,
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
