
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use tentgent_core::{
    daemon::{DaemonInspection, DaemonProcessMetadata},
    server::{LaunchMode, ServerManager, ServerRunRequest},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

use super::*;
use crate::{
    app::DaemonHttpState,
    http::{find_header_end, reason_phrase, HttpBody, HttpRequest, HttpResponse},
    routes::{
        lifecycle::{
            start_server_options, wait_for_server_readiness, READINESS_DEFAULT_TIMEOUT_SECONDS,
        },
        store::path_string,
    },
};

#[tokio::test]
async fn healthz_returns_ok_payload() {
    let request = get("/healthz");
    let state = state_for(unique_home("healthz"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "tentgent-daemon");
}

#[tokio::test]
async fn status_returns_daemon_metadata() {
    let request = get("/v1/status");
    let state = state_for(unique_home("status"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["status"], "running");
    assert_eq!(body["host"], "127.0.0.1");
    assert_eq!(body["port"], 8790);
    assert_eq!(body["pid"], 1234);
}

#[tokio::test]
async fn unknown_route_returns_json_error() {
    let request = get("/v1/missing");
    let state = state_for(unique_home("missing-route"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 404);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn models_returns_empty_array_for_isolated_home() {
    let request = get("/v1/models");
    let state = state_for(unique_home("models-empty"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["models"].as_array().expect("models").len(), 0);
}

#[tokio::test]
async fn adapters_returns_empty_array_for_isolated_home() {
    let request = get("/v1/adapters");
    let state = state_for(unique_home("adapters-empty"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["adapters"].as_array().expect("adapters").len(), 0);
}

#[tokio::test]
async fn datasets_returns_empty_array_for_isolated_home() {
    let request = get("/v1/datasets");
    let state = state_for(unique_home("datasets-empty"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["datasets"].as_array().expect("datasets").len(), 0);
}

#[tokio::test]
async fn servers_returns_stored_server_summaries() {
    let home = unique_home("servers-list");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8791),
            lazy_load: true,
            idle_seconds: Some(60),
        })
        .expect("server spec");

    let request = get("/v1/servers");
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let servers = body["servers"].as_array().expect("servers");

    assert_eq!(response.status_code, 200);
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["server_ref"], outcome.spec.server_ref);
    assert_eq!(servers[0]["runtime_kind"], "cloud");
    assert_eq!(servers[0]["provider"], "openai");
    assert_eq!(servers[0]["provider_model"], "gpt-4.1-mini");
    assert_eq!(servers[0]["running"], false);
}

#[tokio::test]
async fn server_inspect_accepts_short_ref() {
    let home = unique_home("server-inspect");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "anthropic:claude-3-5-sonnet-latest".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8792),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");

    let request = get(&format!("/v1/servers/{}", outcome.spec.short_ref));
    let state = state_for(home.clone());
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["server"]["server_ref"], outcome.spec.server_ref);
    assert_eq!(body["server"]["home_dir"], path_string(&home));
    assert_eq!(body["server"]["runtime_kind"], "cloud");
    assert_eq!(body["server"]["provider"], "anthropic");
}

#[tokio::test]
async fn missing_server_returns_json_404() {
    let request = get("/v1/servers/missing");
    let state = state_for(unique_home("server-missing"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 404);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn server_health_on_stopped_server_returns_unreachable() {
    let home = unique_home("server-health-stopped");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8910),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");
    let request = get(&format!("/v1/servers/{}/health", outcome.spec.short_ref));
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["server"]["server_ref"], outcome.spec.server_ref);
    assert_eq!(body["running"], false);
    assert_eq!(body["reachable"], false);
    assert_eq!(body["target_status"], Value::Null);
    assert!(body["error"]
        .as_str()
        .expect("error")
        .contains("not running"));
}

#[tokio::test]
async fn server_health_on_running_server_reads_target_health() {
    let (port, _received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"status":"ok","chat_ready":true}"#,
        true,
    )
    .await;
    let home = unique_home("server-health-running");
    let server_ref = create_running_cloud_server(&home, port);
    let request = get(&format!("/v1/servers/{server_ref}/health"));
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["running"], true);
    assert_eq!(body["reachable"], true);
    assert_eq!(body["target_status"], 200);
    assert_eq!(body["target_health"]["status"], "ok");
    assert_eq!(body["target_health"]["chat_ready"], true);
}

#[tokio::test]
async fn server_health_missing_ref_returns_404() {
    let request = get("/v1/servers/missing/health");
    let state = state_for(unique_home("server-health-missing"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 404);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn post_servers_creates_cloud_server_spec() {
    let request = post(
            "/v1/servers",
            br#"{"runtime_ref":"openai:gpt-4.1-mini","host":"127.0.0.1","port":8793,"lazy_load":true,"idle_seconds":45}"#,
        );
    let state = state_for(unique_home("server-create"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["created"], true);
    assert_eq!(body["server"]["runtime_kind"], "cloud");
    assert_eq!(body["server"]["provider"], "openai");
    assert_eq!(body["server"]["provider_model"], "gpt-4.1-mini");
    assert_eq!(body["server"]["host"], "127.0.0.1");
    assert_eq!(body["server"]["port"], 8793);
    assert_eq!(body["server"]["lazy_load"], true);
    assert_eq!(body["server"]["idle_seconds"], 45);
}

#[tokio::test]
async fn post_servers_reuses_existing_cloud_server_spec() {
    let home = unique_home("server-reuse");
    let state = state_for(home);
    let request = post(
            "/v1/servers",
            br#"{"runtime_ref":"openai:gpt-4.1-mini","host":"127.0.0.1","port":8794,"lazy_load":false}"#,
        );

    let first = route_request(&request, &state).await;
    let second = route_request(&request, &state).await;
    let first_body: Value = serde_json::from_slice(response_buffer(&first)).expect("json");
    let second_body: Value = serde_json::from_slice(response_buffer(&second)).expect("json");

    assert_eq!(first.status_code, 200);
    assert_eq!(second.status_code, 200);
    assert_eq!(first_body["created"], true);
    assert_eq!(second_body["created"], false);
    assert_eq!(
        first_body["server"]["server_ref"],
        second_body["server"]["server_ref"]
    );
}

#[tokio::test]
async fn post_servers_invalid_json_returns_400() {
    let request = post("/v1/servers", br#"{"runtime_ref":"openai:gpt-4.1-mini""#);
    let state = state_for(unique_home("server-invalid-json"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 400);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn post_servers_missing_runtime_ref_returns_400() {
    let request = post("/v1/servers", br#"{"host":"127.0.0.1"}"#);
    let state = state_for(unique_home("server-missing-runtime"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 400);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn start_missing_server_returns_json_404() {
    let request = post("/v1/servers/missing/start", b"{}");
    let state = state_for(unique_home("server-start-missing"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 404);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn stop_missing_server_returns_json_404() {
    let request = post("/v1/servers/missing/stop", b"{}");
    let state = state_for(unique_home("server-stop-missing"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 404);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn stop_stopped_server_returns_json_409() {
    let home = unique_home("server-stop-stopped");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8795),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");

    let request = post(
        &format!("/v1/servers/{}/stop", outcome.spec.short_ref),
        b"{}",
    );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 409);
    assert_eq!(body["error"], "not_running");
}

#[test]
fn start_options_empty_body_preserves_default_shape() {
    let request = post("/v1/servers/abc/start", b"");
    let options = start_server_options(&request).expect("options");

    assert!(!options.wait_ready);
    assert_eq!(
        options.timeout,
        Duration::from_secs(READINESS_DEFAULT_TIMEOUT_SECONDS)
    );
}

#[tokio::test]
async fn start_invalid_body_returns_400() {
    let request = post("/v1/servers/missing/start", br#"{"wait_ready":"yes"}"#);
    let state = state_for(unique_home("server-start-invalid-body"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 400);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn start_invalid_timeout_returns_400() {
    let request = post(
        "/v1/servers/missing/start",
        br#"{"wait_ready":true,"timeout_seconds":121}"#,
    );
    let state = state_for(unique_home("server-start-invalid-timeout"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 400);
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("timeout_seconds"));
}

#[tokio::test]
async fn wait_readiness_returns_ready_when_target_is_reachable() {
    let (port, _received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"status":"ok","chat_ready":true}"#,
        true,
    )
    .await;
    let home = unique_home("server-readiness-ready");
    let server_ref = create_running_cloud_server(&home, port);
    let manager = ServerManager::open_readonly(Some(&home)).expect("server manager");
    let server = manager.inspect(&server_ref).expect("inspect");
    let state = state_for(home);
    let readiness = wait_for_server_readiness(&state, &server, Duration::from_secs(1)).await;

    assert!(readiness.ready);
    assert!(readiness.reachable);
    assert_eq!(readiness.target_status, Some(200));
    assert_eq!(
        readiness.target_health.expect("target health")["status"],
        "ok"
    );
    assert_eq!(readiness.error, None);
}

#[tokio::test]
async fn wait_readiness_timeout_returns_not_ready() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    let home = unique_home("server-readiness-timeout");
    let server_ref = create_running_cloud_server(&home, port);
    let manager = ServerManager::open_readonly(Some(&home)).expect("server manager");
    let server = manager.inspect(&server_ref).expect("inspect");
    let state = state_for(home);
    let readiness = wait_for_server_readiness(&state, &server, Duration::from_millis(1)).await;

    assert!(!readiness.ready);
    assert!(!readiness.reachable);
    assert_eq!(readiness.target_status, None);
    assert!(readiness
        .error
        .as_deref()
        .expect("error")
        .contains("did not become ready"));
}

#[tokio::test]
async fn chat_without_running_server_returns_409() {
    let request = post(
        "/v1/chat",
        br#"{"messages":[{"role":"user","content":"Hello"}]}"#,
    );
    let state = state_for(unique_home("chat-none-running"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 409);
    assert_eq!(body["error"], "no_running_server");
}

#[tokio::test]
async fn chat_without_server_ref_and_multiple_running_servers_returns_409() {
    let home = unique_home("chat-multiple-running");
    create_running_cloud_server(&home, 8901);
    create_running_cloud_server(&home, 8902);

    let request = post(
        "/v1/chat",
        br#"{"messages":[{"role":"user","content":"Hello"}]}"#,
    );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 409);
    assert_eq!(body["error"], "ambiguous_server");
}

#[tokio::test]
async fn chat_missing_explicit_server_ref_returns_404() {
    let request = post(
        "/v1/chat",
        br#"{"server_ref":"missing","messages":[{"role":"user","content":"Hello"}]}"#,
    );
    let state = state_for(unique_home("chat-missing-server"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 404);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn chat_stopped_explicit_server_ref_returns_409() {
    let home = unique_home("chat-stopped-server");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8903),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");

    let request = post(
        "/v1/chat",
        format!(
            r#"{{"server_ref":"{}","messages":[{{"role":"user","content":"Hello"}}]}}"#,
            outcome.spec.short_ref
        )
        .as_bytes(),
    );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 409);
    assert_eq!(body["error"], "server_not_running");
}

#[tokio::test]
async fn chat_invalid_json_returns_400() {
    let request = post("/v1/chat", br#"{"messages":"#);
    let state = state_for(unique_home("chat-invalid-json"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 400);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn chat_proxy_strips_server_ref_and_returns_target_json() {
    let (port, received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"text":"hello","stream":false}"#,
        true,
    )
    .await;
    let home = unique_home("chat-proxy-json");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
            "/v1/chat",
            format!(
                r#"{{"server_ref":"{}","messages":[{{"role":"user","content":"Hello"}}],"stream":false}}"#,
                server_ref
            )
            .as_bytes(),
        );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let proxied_body: Value =
        serde_json::from_slice(&received_body.await.expect("mock received body"))
            .expect("proxied json");

    assert_eq!(response.status_code, 200);
    assert_eq!(response.content_type, "application/json; charset=utf-8");
    assert_eq!(body["text"], "hello");
    assert!(proxied_body.get("server_ref").is_none());
    assert_eq!(proxied_body["messages"][0]["content"], "Hello");
}

#[tokio::test]
async fn chat_proxy_passes_through_target_error() {
    let (port, _received_body) = spawn_mock_chat_server(
        501,
        "application/json; charset=utf-8",
        br#"{"error":"stream_not_implemented","message":"nope"}"#,
        true,
    )
    .await;
    let home = unique_home("chat-proxy-error");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
        "/v1/chat",
        format!(
            r#"{{"server_ref":"{}","messages":[{{"role":"user","content":"Hello"}}]}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 501);
    assert_eq!(body["error"], "stream_not_implemented");
}

#[tokio::test]
async fn chat_proxy_streams_sse_response() {
    let sse = b"event: delta\ndata: {\"delta\":\"hi\"}\n\nevent: done\ndata: {\"finish_reason\":\"stop\"}\n\n";
    let (port, _received_body) =
        spawn_mock_chat_server(200, "text/event-stream; charset=utf-8", sse, false).await;
    let home = unique_home("chat-proxy-sse");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
            "/v1/chat",
            format!(
                r#"{{"server_ref":"{}","messages":[{{"role":"user","content":"Hello"}}],"stream":true}}"#,
                server_ref
            )
            .as_bytes(),
        );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let content_type = response.content_type.clone();
    let body = collect_response_body(response).await;

    assert_eq!(content_type, "text/event-stream; charset=utf-8");
    assert_eq!(body, sse);
}

#[tokio::test]
async fn chat_proxy_transport_failure_returns_502() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    let home = unique_home("chat-proxy-transport");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
        "/v1/chat",
        format!(
            r#"{{"server_ref":"{}","messages":[{{"role":"user","content":"Hello"}}]}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 502);
    assert_eq!(body["error"], "server_proxy_failed");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("/v1/servers/"));
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("/health"));
}

fn get(path: &str) -> HttpRequest {
    HttpRequest {
        method: "GET".to_string(),
        path: path.to_string(),
        version: "HTTP/1.1".to_string(),
        body: Vec::new(),
        parse_error: None,
    }
}

fn post(path: &str, body: &[u8]) -> HttpRequest {
    HttpRequest {
        method: "POST".to_string(),
        path: path.to_string(),
        version: "HTTP/1.1".to_string(),
        body: body.to_vec(),
        parse_error: None,
    }
}

fn response_buffer(response: &HttpResponse) -> &[u8] {
    match &response.body {
        HttpBody::Buffered(body) => body,
        HttpBody::Proxy(_) => panic!("expected buffered response"),
    }
}

async fn collect_response_body(response: HttpResponse) -> Vec<u8> {
    match response.body {
        HttpBody::Buffered(body) => body,
        HttpBody::Proxy(mut upstream) => {
            let mut body = Vec::new();
            while let Some(chunk) = upstream.chunk().await.expect("chunk") {
                body.extend_from_slice(&chunk);
            }
            body
        }
    }
}

fn create_running_cloud_server(home: &Path, port: u16) -> String {
    let manager = ServerManager::new(Some(home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(port),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");
    manager
        .record_process_start(
            &outcome.spec.server_ref,
            std::process::id(),
            LaunchMode::Background,
        )
        .expect("record process");
    outcome.spec.short_ref
}

async fn spawn_mock_chat_server(
    status: u16,
    content_type: &'static str,
    response_body: &'static [u8],
    include_content_length: bool,
) -> (u16, oneshot::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("local addr").port();
    let (sender, receiver) = oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = stream.read(&mut chunk).await.expect("read");
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let header_end = find_header_end(&buffer).expect("headers");
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().expect("content length"))
            })
            .unwrap_or(0);
        let body_start = header_end + 4;
        let mut body = buffer[body_start..].to_vec();
        while body.len() < content_length {
            let read = stream.read(&mut chunk).await.expect("read body");
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read]);
        }
        body.truncate(content_length);
        let _ = sender.send(body);

        let length_header = if include_content_length {
            format!("Content-Length: {}\r\n", response_body.len())
        } else {
            String::new()
        };
        let cache_header = if content_type.starts_with("text/event-stream") {
            "Cache-Control: no-cache\r\n"
        } else {
            ""
        };
        let response = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n{}{}Connection: close\r\n\r\n",
            status,
            reason_phrase(status),
            content_type,
            cache_header,
            length_header
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write headers");
        stream.write_all(response_body).await.expect("write body");
        stream.shutdown().await.expect("shutdown");
    });

    (port, receiver)
}

fn state_for(home: PathBuf) -> DaemonHttpState {
    DaemonHttpState::new(inspection(home))
}

fn inspection(home: PathBuf) -> DaemonInspection {
    DaemonInspection {
        home_dir: home.clone(),
        runtime_dir: home.join("runtime"),
        log_dir: home.join("logs"),
        process_path: home.join("runtime/daemon.toml"),
        pid_path: home.join("runtime/tentgent.pid"),
        stdout_log_path: home.join("logs/daemon.stdout.log"),
        stderr_log_path: home.join("logs/daemon.stderr.log"),
        running: true,
        process: Some(DaemonProcessMetadata {
            pid: 1234,
            host: "127.0.0.1".to_string(),
            port: 8790,
            started_at: "2026-05-01T00:00:00Z".to_string(),
        }),
    }
}

fn unique_home(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-http-test-{label}-{nanos}"))
}
