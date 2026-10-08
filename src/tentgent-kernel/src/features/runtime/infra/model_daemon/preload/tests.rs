use super::*;
use crate::features::runtime::infra::ModelRuntimeCapability;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn mock_runtime(
    requests: usize,
    reply: impl Fn(serde_json::Value) -> (u16, serde_json::Value) + Send + 'static,
) -> (ModelRuntimeDaemonEndpoint, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = tokio::spawn(async move {
        for _ in 0..requests {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let (header_end, length) = loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                if let Some(end) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    assert!(headers.starts_with("POST /v1/lifecycle/preload HTTP/1.1"));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break (end + 4, length);
                    }
                }
            };
            let payload =
                serde_json::from_slice(&request[header_end..header_end + length]).unwrap();
            let (status, body) = reply(payload);
            let body = body.to_string();
            socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    (
        ModelRuntimeDaemonEndpoint {
            base_url: format!("http://127.0.0.1:{port}"),
            host: "127.0.0.1".into(),
            port,
            pid: 123,
            process_token: "expected-token".into(),
            capability: ModelRuntimeCapability::Chat,
            model_ref: Some("bound-model".into()),
            policy_mismatch: None,
        },
        worker,
    )
}

fn completion(task_ref: &str) -> serde_json::Value {
    json!({"status":"done", "task_ref":task_ref, "model_ref":"bound-model", "capability":"chat", "process_token":"expected-token"})
}

#[tokio::test]
async fn preloads_each_reuse_with_a_unique_task_and_validates_bound_identity() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = seen.clone();
    let (endpoint, worker) = mock_runtime(2, move |body| {
        assert_eq!(body.as_object().unwrap().len(), 2);
        assert_eq!(body["process_token"], "expected-token");
        let task = body["task_ref"].as_str().unwrap();
        captured.lock().unwrap().push(task.to_string());
        (200, completion(task))
    })
    .await;
    for _ in 0..2 {
        assert_eq!(
            preload_model_runtime(&endpoint).await.unwrap().status,
            "done"
        );
    }
    worker.await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_ne!(seen[0], seen[1]);
}

#[tokio::test]
async fn rejects_incomplete_or_wrong_generation_task_model_capability_completions() {
    for field in [
        "status",
        "task_ref",
        "model_ref",
        "capability",
        "process_token",
    ] {
        let (endpoint, worker) = mock_runtime(1, move |_| {
            let mut value = completion("task");
            value[field] = json!("wrong");
            (200, value)
        })
        .await;
        let error = preload_with_client(&reqwest::Client::new(), &endpoint, "task")
            .await
            .unwrap_err();
        assert_eq!(error.kind, ModelRuntimePreloadFailureKind::Unavailable);
        assert!(!error.message.contains("expected-token"));
        worker.await.unwrap();
    }
    let (endpoint, worker) = mock_runtime(1, |_| (200, json!({"status":"done"}))).await;
    assert!(preload_model_runtime(&endpoint).await.is_err());
    worker.await.unwrap();
}

#[tokio::test]
async fn distinguishes_terminal_loading_from_observation_and_admission_failures() {
    for (status, code, accepted, expected) in [
        (
            500,
            "preload_failed",
            true,
            ModelRuntimePreloadFailureKind::LoadFailed,
        ),
        (
            501,
            "preload_unsupported",
            true,
            ModelRuntimePreloadFailureKind::LoadFailed,
        ),
        (
            501,
            "preload_unsupported",
            false,
            ModelRuntimePreloadFailureKind::Unavailable,
        ),
        (
            504,
            "preload_wait_timeout",
            true,
            ModelRuntimePreloadFailureKind::ObservationTimeout,
        ),
        (
            409,
            "runtime_generation_mismatch",
            false,
            ModelRuntimePreloadFailureKind::Unavailable,
        ),
        (
            500,
            "preload_failed",
            false,
            ModelRuntimePreloadFailureKind::Unavailable,
        ),
    ] {
        let (endpoint, worker) = mock_runtime(1, move |_| (status, json!({"detail":{
            "code":code, "message":"test failure", "task_ref":if accepted {"task"} else {"other"}
        }}))).await;
        let error = preload_with_client(&reqwest::Client::new(), &endpoint, "task")
            .await
            .unwrap_err();
        assert_eq!(error.kind, expected);
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn old_runtime_returns_actionable_bootstrap_diagnostic() {
    let (endpoint, worker) = mock_runtime(1, |_| (404, json!({}))).await;
    let error = preload_model_runtime(&endpoint).await.unwrap_err();
    assert!(error
        .message
        .contains("tentgent runtime bootstrap --profile local-model"));
    assert_eq!(error.kind, ModelRuntimePreloadFailureKind::NotAccepted);
    worker.await.unwrap();
}

#[tokio::test]
async fn explicit_pre_admission_rejection_is_safe_to_release_without_proof() {
    for (status, code, expected) in [
        (
            400,
            "preload_unbound_runtime",
            ModelRuntimePreloadFailureKind::NotAccepted,
        ),
        (
            409,
            "runtime_generation_mismatch",
            ModelRuntimePreloadFailureKind::NotAccepted,
        ),
        (
            409,
            "runtime_closing",
            ModelRuntimePreloadFailureKind::NotAccepted,
        ),
        (
            422,
            "invalid_preload_request",
            ModelRuntimePreloadFailureKind::NotAccepted,
        ),
        (
            501,
            "preload_unsupported",
            ModelRuntimePreloadFailureKind::NotAccepted,
        ),
        // An existing same-ID task or an unrecognized response may still own work.
        (
            409,
            "preload_task_exists",
            ModelRuntimePreloadFailureKind::Unavailable,
        ),
        (503, "unknown", ModelRuntimePreloadFailureKind::Unavailable),
    ] {
        let (endpoint, worker) = mock_runtime(1, move |_| {
            (
                status,
                json!({"detail": {"code": code, "message": "rejected"}}),
            )
        })
        .await;
        let error = preload_model_runtime(&endpoint).await.unwrap_err();
        assert_eq!(error.kind, expected, "{code}");
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn transport_timeout_is_not_load_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let pending = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let endpoint = ModelRuntimeDaemonEndpoint {
        base_url: format!("http://127.0.0.1:{port}"),
        host: "127.0.0.1".into(),
        port,
        pid: 1,
        process_token: "token".into(),
        capability: ModelRuntimeCapability::Chat,
        model_ref: Some("model".into()),
        policy_mismatch: None,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(25))
        .build()
        .unwrap();
    let error = preload_with_client(&client, &endpoint, "task")
        .await
        .unwrap_err();
    assert_eq!(
        error.kind,
        ModelRuntimePreloadFailureKind::ObservationTimeout
    );
    assert!(!pending.is_finished());
    pending.abort();
}
