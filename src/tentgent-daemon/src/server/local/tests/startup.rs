use super::*;
use crate::server::local::{
    runtime::{local_router, serve_with_startup},
    startup::{prepare_with_endpoint, StartupReadiness},
    LocalServerRuntimeConfig, LocalServerState,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tentgent_kernel::features::{
    model::{
        domain::{
            default_model_capability_source, ModelCapability, ModelCapabilityProof,
            ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelFormat, ModelMetadata,
            ModelRef, ModelSourceKind, ModelStoreLayout,
        },
        infra::{FileModelCapabilityProofStore, FileModelCatalogStore},
        ports::{ModelCapabilityProofStore, ModelCatalogStore},
    },
    runtime::{
        domain::{PythonRuntimeLayout, PythonRuntimeSource},
        infra::{
            ModelRuntimeCapability, ModelRuntimeDaemonEndpoint, ModelRuntimeDaemonLaunchPolicy,
            ModelRuntimeDaemonSupervisor, StdRuntimeExecutableResolver,
        },
    },
    server::options::LoadMode,
};
use tokio::sync::Notify;
use tower::ServiceExt;

fn state(label: &str, mode: LoadMode) -> LocalServerState {
    let layout = test_runtime_layout(label);
    let model_ref = ModelRef::parse("7".repeat(64)).unwrap();
    FileModelCatalogStore
        .save_model_metadata(
            &ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
            &ModelMetadata {
                model_ref: model_ref.clone(),
                short_ref: model_ref.short_ref().into(),
                source_kind: ModelSourceKind::Local,
                source_repo: None,
                source_revision: None,
                source_path: None,
                primary_format: ModelFormat::Safetensors,
                detected_formats: vec![ModelFormat::Safetensors],
                mlx_runtime_family: None,
                model_capabilities: vec![ModelCapability::Chat],
                model_capability_source: default_model_capability_source(),
                file_count: 1,
                total_bytes: 1,
                imported_at: "2026-09-28T00:00:00Z".into(),
            },
        )
        .unwrap();
    LocalServerState {
        config: LocalServerRuntimeConfig {
            server_ref: "server-ref".into(),
            capability: ServerCapability::Chat,
            model_ref: model_ref.to_string(),
            runtime_profile: Some("local-chat-transformers-peft-v1".into()),
            host: "127.0.0.1".into(),
            port: 0,
            runtime_home: Some(layout.home_dir.clone()),
            runtime_idle_seconds: 300,
            model_idle_seconds: 0,
            load_mode: mode,
        },
        runtime: PythonRuntimeLayout {
            project_dir: layout.runtime_dir.join("no-python-needed"),
            env_dir: layout.python_env_dir.clone(),
            source: PythonRuntimeSource::DevelopmentSource,
        },
        layout,
        executable_resolver: StdRuntimeExecutableResolver,
        supervisor: ModelRuntimeDaemonSupervisor::new(),
        client: reqwest::Client::new(),
        launch_policy: ModelRuntimeDaemonLaunchPolicy::default(),
        readiness: StartupReadiness::new(mode),
    }
}

fn proofs(state: &LocalServerState) -> Vec<ModelCapabilityProof> {
    FileModelCapabilityProofStore
        .list_capability_proofs(
            &ModelStoreLayout::from_models_dir(state.layout.models_dir.clone()),
            &ModelRef::parse(&state.config.model_ref).unwrap(),
        )
        .unwrap()
}

async fn endpoint(
    state: &LocalServerState,
    status: StatusCode,
    gate: Option<Arc<Notify>>,
) -> (
    ModelRuntimeDaemonEndpoint,
    tokio::task::JoinHandle<()>,
    Arc<AtomicUsize>,
) {
    let count = Arc::new(AtomicUsize::new(0));
    let requested = count.clone();
    let model_ref = state.config.model_ref.clone();
    let (base_url, worker) = spawn_test_server(Router::new().route("/v1/lifecycle/preload", post(
        move |Json(payload): Json<Value>| {
            let (model_ref, requested, gate) = (model_ref.clone(), requested.clone(), gate.clone());
            async move {
                requested.fetch_add(1, Ordering::SeqCst);
                if let Some(gate) = gate { gate.notified().await; }
                let body = if status == StatusCode::OK {
                    json!({"status":"done", "task_ref":payload["task_ref"], "process_token":payload["process_token"],
                        "model_ref":model_ref, "capability":"chat"})
                } else {
                    json!({"detail":{"code": if status == StatusCode::GATEWAY_TIMEOUT {"preload_wait_timeout"} else {"preload_failed"},
                        "message":"fixture failure", "task_ref":payload["task_ref"]}})
                };
                (status, Json(body))
            }
        }
    ))).await;
    (
        ModelRuntimeDaemonEndpoint {
            base_url,
            host: "127.0.0.1".into(),
            port: 0,
            pid: 123,
            process_token: "python-generation".into(),
            capability: ModelRuntimeCapability::Chat,
            model_ref: Some(state.config.model_ref.clone()),
            policy_mismatch: Some("reuse must not change first-spawner policy".into()),
        },
        worker,
        count,
    )
}

#[tokio::test]
async fn lazy_start_never_resolves_or_preloads_a_runtime_and_writes_no_proof() {
    let state = state("startup-lazy", LoadMode::Lazy);
    prepare_with_endpoint(&state, async {
        panic!("lazy startup must not resolve Python")
    })
    .await
    .unwrap();
    assert!(state.readiness.is_ready());
    assert!(proofs(&state).is_empty());
}

#[tokio::test]
async fn eager_first_start_and_reuse_both_preload_and_record_only_completion() {
    let state = state("startup-eager-reuse", LoadMode::Eager);
    let gate = Arc::new(Notify::new());
    let (endpoint, worker, count) = endpoint(&state, StatusCode::OK, Some(gate.clone())).await;
    for _ in 0..2 {
        assert!(!state.readiness.is_ready());
        let startup = prepare_with_endpoint(&state, std::future::ready(Ok(endpoint.clone())));
        tokio::pin!(startup);
        tokio::select! {
            result = &mut startup => panic!("must wait for model completion: {result:?}"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {}
        }
        if count.load(Ordering::SeqCst) == 1 {
            assert!(proofs(&state).is_empty());
        }
        gate.notify_one();
        startup.await.unwrap();
        let proofs = proofs(&state);
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].status, ModelCapabilityProofStatus::Verified);
        assert_eq!(proofs[0].source, ModelCapabilityProofSource::ServerStart);
        assert_eq!(
            proofs[0].runtime_profile.as_deref(),
            Some("local-chat-transformers-peft")
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(!worker.is_finished()); // Startup never kills the shared runtime.
    worker.abort();
}

#[tokio::test]
async fn terminal_failure_records_failed_but_timeout_or_old_runtime_do_not() {
    for status in [
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::GATEWAY_TIMEOUT,
        StatusCode::NOT_FOUND,
    ] {
        let state = state(&format!("startup-{status}"), LoadMode::Eager);
        let (endpoint, worker, _) = endpoint(&state, status, None).await;
        let error = prepare_with_endpoint(&state, std::future::ready(Ok(endpoint)))
            .await
            .unwrap_err();
        assert!(!state.readiness.is_ready());
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            assert_eq!(proofs(&state)[0].status, ModelCapabilityProofStatus::Failed);
        } else {
            assert!(proofs(&state).is_empty());
        }
        if status == StatusCode::NOT_FOUND {
            assert!(error.message.contains("runtime bootstrap"));
        }
        assert!(!worker.is_finished());
        worker.abort();
    }
}

#[tokio::test]
async fn starting_health_is_observational_and_admission_blocks_every_inference_family() {
    let state = state("startup-admission", LoadMode::Eager);
    let router = local_router(state.clone());
    for path in [
        "/v1/chat",
        "/v1/chat/stream",
        "/v1/chat/completions",
        "/v1/messages",
        "/v1beta/models/test:generateContent",
        "/v1/embeddings",
        "/v1/rerank",
        "/v1/audio/transcriptions",
        "/v1/images/generations",
    ] {
        let response = router
            .clone()
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
    for path in [
        "/v1/lifecycle/preload",
        "/internal/v1/chat",
        "/v1/chat/",
        "/openapi.json",
    ] {
        let private = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(private.status(), StatusCode::NOT_FOUND);
    }
    let health = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let health: Value =
        serde_json::from_slice(&to_bytes(health.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(health["status"], "starting");
    assert_eq!(health["ready"], false);
    assert!(proofs(&state).is_empty());
    assert!(!state
        .layout
        .runtime_dir
        .join("model-runtime-daemons")
        .exists());
    state.readiness.mark_ready();
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::SERVICE_UNAVAILABLE); // Normal body validation now runs.
}

#[tokio::test]
async fn listener_exposes_starting_then_ready_and_exits_on_startup_failure() {
    for succeeds in [true, false] {
        let state = state(&format!("startup-listener-{succeeds}"), LoadMode::Eager);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/healthz", listener.local_addr().unwrap());
        let (complete, pending) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_with_startup(state, listener, async move {
            pending.await.unwrap()
        }));
        let client = reqwest::Client::new();
        let health: Value = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(health["ready"], false);
        complete
            .send(if succeeds {
                Ok(())
            } else {
                Err(crate::server::local::LocalServerError::internal(
                    "load failed".into(),
                ))
            })
            .unwrap();
        if succeeds {
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    let health: Value =
                        client.get(&url).send().await.unwrap().json().await.unwrap();
                    if health["ready"] == true {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            server.abort();
        } else {
            assert!(server.await.unwrap().is_err());
            assert!(client.get(&url).send().await.is_err());
        }
    }
}
