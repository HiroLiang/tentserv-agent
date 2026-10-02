use super::*;
use crate::server::local::{
    ingress::{ensure_public_runtime_request, PUBLIC_RUNTIME_PATHS},
    proxy::proxy_request,
    LocalServerRuntimeConfig, LocalServerState,
};
use tentgent_kernel::features::runtime::{
    domain::{PythonRuntimeLayout, PythonRuntimeSource},
    infra::{
        ModelRuntimeDaemonLaunchPolicy, ModelRuntimeDaemonSupervisor, StdRuntimeExecutableResolver,
    },
};

const PRIVATE_PATHS: &[&str] = &[
    "/v1/lifecycle",
    "/v1/lifecycle/",
    "/v1/lifecycle/preload",
    "/v1/lifecycle/shutdown?ignored=true",
    "/v1/lifecycle/future-operation",
    "/internal/v1/lifecycle/preload",
    "/internal/v1/lifecycle/shutdown/",
    "/v1/%6cifecycle/preload",
    "/%76%31%2Flifecycle%2Fpreload",
    "/internal%2fv1%2flifecycle%2fshutdown",
    "//v1//lifecycle//preload",
    "/prefix/../v1/lifecycle/preload",
    "/prefix/%2E%2E/v1/lifecycle/preload",
    "/v1%5Clifecycle%5cshutdown",
    "/v1/lifecycle/preload/%2f../..",
    "/v1/lifecycle/shutdown/%2F../..",
    "/internal/v1/chat",
    "/internal/v1/chat/stream",
    "/v1/chat/",
    "/v1/%63hat",
    "/prefix/../v1/chat",
    "/v1/chat/stream/",
    "/v1/embeddings/",
    "/v1/images/generations/",
    "/healthz/",
    "/%68ealthz",
    "/openapi.json",
    "/v1/tuning/lora/runs",
];

#[test]
fn forwarding_guard_accepts_only_exact_native_post_routes() {
    for path in PUBLIC_RUNTIME_PATHS {
        assert!(
            ensure_public_runtime_request(&Method::POST, path).is_ok(),
            "{path}"
        );
        for method in [Method::GET, Method::DELETE, Method::PUT] {
            assert_eq!(
                ensure_public_runtime_request(&method, path)
                    .unwrap_err()
                    .status,
                StatusCode::METHOD_NOT_ALLOWED,
            );
        }
    }
    for path in [
        "/v1/chat",
        "/v1/chat/stream",
        "/v1/embeddings",
        "/v1/images/generations",
    ] {
        assert!(
            ensure_public_runtime_request(&Method::POST, path).is_err(),
            "managed handler required: {path}"
        );
    }
    let uri: axum::http::Uri = "/v1/rerank?text=/v1/lifecycle/preload".parse().unwrap();
    assert!(ensure_public_runtime_request(&Method::POST, uri.path()).is_ok());
    assert_eq!(
        runtime_upstream_path_and_query(&uri.to_string()),
        "/internal/v1/rerank?text=/v1/lifecycle/preload"
    );
}

#[tokio::test]
async fn private_requests_fail_before_runtime_or_proof_side_effects() {
    use tower::ServiceExt;
    let layout = test_runtime_layout("lifecycle-ingress");
    let state = LocalServerState {
        config: LocalServerRuntimeConfig {
            server_ref: "must-not-start".into(),
            capability: ServerCapability::Chat,
            model_ref: "missing-model".into(),
            runtime_profile: None,
            host: "127.0.0.1".into(),
            port: 0,
            runtime_home: Some(layout.home_dir.clone()),
            runtime_idle_seconds: 300,
            model_idle_seconds: 0,
            load_mode: tentgent_kernel::features::server::options::LoadMode::Lazy,
        },
        runtime: PythonRuntimeLayout {
            project_dir: layout.runtime_dir.join("missing-project"),
            env_dir: layout.python_env_dir.clone(),
            source: PythonRuntimeSource::DevelopmentSource,
        },
        layout: layout.clone(),
        executable_resolver: StdRuntimeExecutableResolver,
        supervisor: ModelRuntimeDaemonSupervisor::new(),
        client: reqwest::Client::new(),
        launch_policy: ModelRuntimeDaemonLaunchPolicy::default(),
        readiness: crate::server::local::startup::StartupReadiness::new(
            tentgent_kernel::features::server::options::LoadMode::Lazy,
        ),
    };
    let before = tree_contents(&layout.home_dir);
    for path in PRIVATE_PATHS {
        for method in [Method::GET, Method::POST, Method::DELETE] {
            let request = Request::builder()
                .method(method.clone())
                .uri(*path)
                .body(Body::from("not even valid JSON"))
                .unwrap();
            let error = proxy_request(AxumState(state.clone()), request)
                .await
                .expect_err("lifecycle must fail before resolving the missing model/runtime");
            assert_eq!(error.status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(error.code, "not_found");
            let response = crate::server::local::runtime::local_router(state.clone())
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(*path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "router: {path}");
        }
    }
    assert_eq!(
        tree_contents(&layout.home_dir),
        before,
        "no runtime, generation, or proof writes"
    );
}

fn tree_contents(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            entries.push((path.clone(), Vec::new()));
            entries.extend(tree_contents(&path));
        } else {
            entries.push((path.clone(), std::fs::read(&path).unwrap()));
        }
    }
    entries.sort();
    entries
}
