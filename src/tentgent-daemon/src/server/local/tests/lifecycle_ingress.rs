use super::*;
use crate::server::local::{
    ingress::reject_lifecycle_path, proxy::proxy_request, LocalServerRuntimeConfig,
    LocalServerState,
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
];

#[test]
fn lifecycle_guard_keeps_inference_paths_and_queries_unchanged() {
    for path in [
        "/v1/chat",
        "/v1/chat/stream",
        "/v1/embeddings",
        "/v1/rerank",
        "/internal/v1/audio/speech",
        "/v1/images/generations",
        "/v1/lifecycle-other",
    ] {
        assert!(reject_lifecycle_path(path).is_ok(), "{path}");
    }
    let uri: axum::http::Uri = "/v1/chat?text=/v1/lifecycle/preload".parse().unwrap();
    assert!(reject_lifecycle_path(uri.path()).is_ok());
    assert_eq!(
        runtime_upstream_path_and_query(&uri.to_string()),
        uri.to_string()
    );
}

#[tokio::test]
async fn lifecycle_requests_fail_before_runtime_or_proof_side_effects() {
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
                .method(method)
                .uri(*path)
                .body(Body::from("not even valid JSON"))
                .unwrap();
            let error = proxy_request(AxumState(state.clone()), request)
                .await
                .expect_err("lifecycle must fail before resolving the missing model/runtime");
            assert_eq!(error.status, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(error.code, "not_found");
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
