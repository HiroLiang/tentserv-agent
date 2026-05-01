use std::{
    fs,
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
    security::DaemonSecurityConfig,
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
    assert_eq!(body["auth"]["token_enabled"], false);
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
async fn token_enabled_keeps_healthz_public_but_protects_v1_routes() {
    let state = state_with_token(unique_home("auth-protected"), "secret");

    let health = route_request(&get("/healthz"), &state).await;
    assert_eq!(health.status_code, 200);

    for request in [
        get("/v1/status"),
        get_with_header("/v1/status", "Authorization", "Basic secret"),
        get_with_header("/v1/status", "Authorization", "Bearer wrong"),
    ] {
        let response = route_request(&request, &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

        assert_eq!(response.status_code, 401);
        assert_eq!(body["error"], "unauthorized");
        assert_eq!(body["message"], "missing or invalid daemon bearer token");
        assert_eq!(
            response_header(&response, "WWW-Authenticate"),
            Some("Bearer")
        );
    }

    let authorized = route_request(
        &get_with_header("/v1/status", "Authorization", "Bearer secret"),
        &state,
    )
    .await;
    let authorized_body: Value =
        serde_json::from_slice(response_buffer(&authorized)).expect("json");

    assert_eq!(authorized.status_code, 200);
    assert_eq!(authorized_body["auth"]["token_enabled"], true);
}

#[tokio::test]
async fn token_enabled_authenticates_unknown_v1_routes_before_404() {
    let state = state_with_token(unique_home("auth-unknown-route"), "secret");

    let unauthorized = route_request(&get("/v1/not-real"), &state).await;
    let unauthorized_body: Value =
        serde_json::from_slice(response_buffer(&unauthorized)).expect("json");
    assert_eq!(unauthorized.status_code, 401);
    assert_eq!(unauthorized_body["error"], "unauthorized");

    let authorized = route_request(
        &get_with_header("/v1/not-real", "Authorization", "Bearer secret"),
        &state,
    )
    .await;
    let authorized_body: Value =
        serde_json::from_slice(response_buffer(&authorized)).expect("json");
    assert_eq!(authorized.status_code, 404);
    assert_eq!(authorized_body["error"], "not_found");
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
async fn sessions_returns_empty_array_for_isolated_home() {
    let request = get("/v1/sessions");
    let state = state_for(unique_home("sessions-empty"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["sessions"].as_array().expect("sessions").len(), 0);
}

#[tokio::test]
async fn session_list_inspect_and_messages_tail_work() {
    let home = unique_home("sessions-fixture");
    write_session_fixture(
        &home,
        "111111111111000000000000",
        "Older",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        0,
        None,
    );
    write_session_fixture(
        &home,
        "222222222222000000000000",
        "Newer",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:20:00Z",
        3,
        Some(&[
            session_message("user", "one"),
            session_message("assistant", "two"),
            session_message("user", "three"),
        ]),
    );
    let state = state_for(home.clone());

    let list = route_request(&get("/v1/sessions"), &state).await;
    let list_body: Value = serde_json::from_slice(response_buffer(&list)).expect("json");
    let sessions = list_body["sessions"].as_array().expect("sessions");
    assert_eq!(list.status_code, 200);
    assert_eq!(sessions[0]["short_ref"], "222222222222");
    assert_eq!(sessions[0]["title"], "Newer");
    assert_eq!(
        sessions[0]["store_path"],
        path_string(&home.join("sessions/222222222222000000000000"))
    );

    let inspect = route_request(&get("/v1/sessions/222222"), &state).await;
    let inspect_body: Value = serde_json::from_slice(response_buffer(&inspect)).expect("json");
    assert_eq!(inspect.status_code, 200);
    assert_eq!(
        inspect_body["session"]["session_ref"],
        "222222222222000000000000"
    );
    assert_eq!(
        inspect_body["session"]["messages_path"],
        path_string(&home.join("sessions/222222222222000000000000/messages.jsonl"))
    );
    assert!(inspect_body["session"]["warnings"]
        .as_array()
        .expect("warnings")
        .is_empty());

    let messages = route_request(
        &get("/v1/sessions/222222/messages?tail=2&ignored=true"),
        &state,
    )
    .await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    let messages_array = messages_body["messages"].as_array().expect("messages");
    assert_eq!(messages.status_code, 200);
    assert_eq!(messages_body["tail"], 2);
    assert_eq!(messages_body["total_messages"], 3);
    assert_eq!(messages_body["truncated"], true);
    assert_eq!(messages_array.len(), 2);
    assert_eq!(messages_array[0]["index"], 1);
    assert_eq!(messages_array[0]["role"], "assistant");
    assert_eq!(messages_array[0]["metadata"], serde_json::json!({}));
}

#[tokio::test]
async fn session_missing_messages_returns_warning() {
    let home = unique_home("sessions-missing-messages");
    write_session_fixture(
        &home,
        "333333333333000000000000",
        "Missing messages",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        2,
        None,
    );
    let state = state_for(home);

    let response = route_request(&get("/v1/sessions/333333/messages"), &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["total_messages"], 0);
    assert_eq!(body["truncated"], false);
    assert_eq!(body["warnings"][0]["code"], "messages_missing");
}

#[tokio::test]
async fn session_refs_tail_auth_and_methods_map_errors() {
    let home = unique_home("sessions-errors");
    write_session_fixture(
        &home,
        "aaaaaaaaaaaa000000000000",
        "One",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        0,
        None,
    );
    write_session_fixture(
        &home,
        "aaaaaaaaaaab000000000000",
        "Two",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:20:00Z",
        0,
        None,
    );
    let state = state_for(home);

    let missing = route_request(&get("/v1/sessions/missing"), &state).await;
    let missing_body: Value = serde_json::from_slice(response_buffer(&missing)).expect("json");
    assert_eq!(missing.status_code, 404);
    assert_eq!(missing_body["error"], "not_found");

    let ambiguous = route_request(&get("/v1/sessions/aaaa"), &state).await;
    let ambiguous_body: Value = serde_json::from_slice(response_buffer(&ambiguous)).expect("json");
    assert_eq!(ambiguous.status_code, 409);
    assert_eq!(ambiguous_body["error"], "ambiguous_ref");

    for path in [
        "/v1/sessions/aaaaaaaaaaaa/messages?tail=0",
        "/v1/sessions/aaaaaaaaaaaa/messages?tail=-1",
        "/v1/sessions/aaaaaaaaaaaa/messages?tail=1001",
        "/v1/sessions/aaaaaaaaaaaa/messages?tail=abc",
        "/v1/sessions/aaaaaaaaaaaa/messages?tail=1&tail=2",
    ] {
        let response = route_request(&get(path), &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400, "{path}");
        assert_eq!(body["error"], "bad_request");
    }

    let unknown = route_request(&get("/v1/sessions/aaaaaaaaaaaa/unknown"), &state).await;
    assert_eq!(unknown.status_code, 404);

    let method = route_request(&post("/v1/sessions", b"{}"), &state).await;
    assert_eq!(method.status_code, 405);

    let token_state = state_with_token(unique_home("sessions-token"), "secret");
    let unauthorized = route_request(&get("/v1/sessions"), &token_state).await;
    assert_eq!(unauthorized.status_code, 401);
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
async fn daemon_log_metadata_ignores_query_params() {
    let home = unique_home("daemon-log-metadata");
    fs::create_dir_all(home.join("logs")).expect("logs dir");
    fs::write(home.join("logs/daemon.stdout.log"), b"daemon stdout").expect("stdout log");

    let request = get("/v1/daemon/logs?tail_bytes=0");
    let state = state_for(home.clone());
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["logs"]["stdout"]["kind"], "stdout");
    assert_eq!(body["logs"]["stdout"]["exists"], true);
    assert_eq!(body["logs"]["stdout"]["total_bytes"], 13);
    assert!(body["logs"]["stdout"]["modified_at"].is_string());
    assert_eq!(
        body["logs"]["stdout"]["path"],
        path_string(&home.join("logs/daemon.stdout.log"))
    );
    assert_eq!(body["logs"]["stderr"]["kind"], "stderr");
    assert_eq!(body["logs"]["stderr"]["exists"], false);
    assert_eq!(body["logs"]["stderr"]["total_bytes"], 0);
    assert_eq!(body["logs"]["stderr"]["modified_at"], Value::Null);
}

#[tokio::test]
async fn daemon_log_content_tails_bytes_with_shared_shape() {
    let home = unique_home("daemon-log-content");
    fs::create_dir_all(home.join("logs")).expect("logs dir");
    fs::write(home.join("logs/daemon.stdout.log"), b"line1\nline2\n").expect("stdout log");

    let request = get("/v1/daemon/logs/stdout?tail_bytes=5");
    let state = state_for(home.clone());
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["log"]["owner"], "daemon");
    assert_eq!(body["log"]["server_ref"], Value::Null);
    assert_eq!(body["log"]["short_ref"], Value::Null);
    assert_eq!(body["log"]["kind"], "stdout");
    assert_eq!(
        body["log"]["path"],
        path_string(&home.join("logs/daemon.stdout.log"))
    );
    assert_eq!(body["log"]["exists"], true);
    assert_eq!(body["log"]["total_bytes"], 12);
    assert_eq!(body["log"]["tail_bytes"], 5);
    assert_eq!(body["log"]["truncated"], true);
    assert_eq!(body["log"]["encoding"], "utf-8-lossy");
    assert_eq!(body["log"]["content"], "ine2\n");
}

#[tokio::test]
async fn daemon_missing_log_content_returns_empty_200() {
    let request = get("/v1/daemon/logs/stderr");
    let state = state_for(unique_home("daemon-log-missing"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["log"]["owner"], "daemon");
    assert_eq!(body["log"]["kind"], "stderr");
    assert_eq!(body["log"]["exists"], false);
    assert_eq!(body["log"]["total_bytes"], 0);
    assert_eq!(body["log"]["modified_at"], Value::Null);
    assert_eq!(body["log"]["tail_bytes"], 65536);
    assert_eq!(body["log"]["truncated"], false);
    assert_eq!(body["log"]["content"], "");
}

#[tokio::test]
async fn log_content_rejects_invalid_tail_bytes() {
    for query in [
        "tail_bytes=0",
        "tail_bytes=-1",
        "tail_bytes=abc",
        "tail_bytes=262145",
        "tail_bytes=1&tail_bytes=2",
    ] {
        let request = get(&format!("/v1/daemon/logs/stdout?{query}"));
        let state = state_for(unique_home("daemon-log-invalid-tail"));
        let response = route_request(&request, &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

        assert_eq!(response.status_code, 400, "query: {query}");
        assert_eq!(body["error"], "bad_request");
        assert!(body["message"]
            .as_str()
            .expect("message")
            .contains("tail_bytes"));
    }
}

#[tokio::test]
async fn server_log_metadata_and_content_accept_full_ref_and_short_ref() {
    let home = unique_home("server-log-content");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8911),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");
    fs::write(&outcome.stdout_log_path, b"abcdef").expect("stdout log");

    let state = state_for(home.clone());
    let metadata = route_request(
        &get(&format!("/v1/servers/{}/logs", outcome.spec.server_ref)),
        &state,
    )
    .await;
    let metadata_body: Value =
        serde_json::from_slice(response_buffer(&metadata)).expect("metadata json");

    assert_eq!(metadata.status_code, 200);
    assert_eq!(metadata_body["logs"]["stdout"]["exists"], true);
    assert_eq!(metadata_body["logs"]["stdout"]["total_bytes"], 6);
    assert_eq!(metadata_body["logs"]["stderr"]["exists"], false);

    let content = route_request(
        &get(&format!(
            "/v1/servers/{}/logs/stdout?tail_bytes=2",
            outcome.spec.short_ref
        )),
        &state,
    )
    .await;
    let content_body: Value =
        serde_json::from_slice(response_buffer(&content)).expect("content json");

    assert_eq!(content.status_code, 200);
    assert_eq!(content_body["log"]["owner"], "server");
    assert_eq!(content_body["log"]["server_ref"], outcome.spec.server_ref);
    assert_eq!(content_body["log"]["short_ref"], outcome.spec.short_ref);
    assert_eq!(content_body["log"]["kind"], "stdout");
    assert_eq!(content_body["log"]["tail_bytes"], 2);
    assert_eq!(content_body["log"]["truncated"], true);
    assert_eq!(content_body["log"]["content"], "ef");
}

#[tokio::test]
async fn server_log_missing_and_ambiguous_refs_keep_server_error_mapping() {
    let missing = route_request(
        &get("/v1/servers/missing/logs/stderr"),
        &state_for(unique_home("server-log-missing-ref")),
    )
    .await;
    let missing_body: Value = serde_json::from_slice(response_buffer(&missing)).expect("json");
    assert_eq!(missing.status_code, 404);
    assert_eq!(missing_body["error"], "not_found");

    let home = unique_home("server-log-ambiguous-ref");
    write_cloud_server_spec(
        &home,
        "abcdef1111111111111111111111111111111111111111111111111111111111",
    );
    write_cloud_server_spec(
        &home,
        "abcdef2222222222222222222222222222222222222222222222222222222222",
    );

    let ambiguous = route_request(&get("/v1/servers/abcdef/logs"), &state_for(home)).await;
    let ambiguous_body: Value = serde_json::from_slice(response_buffer(&ambiguous)).expect("json");
    assert_eq!(ambiguous.status_code, 409);
    assert_eq!(ambiguous_body["error"], "ambiguous_ref");
}

#[tokio::test]
async fn unknown_log_kinds_return_json_404() {
    let daemon = route_request(
        &get("/v1/daemon/logs/combined"),
        &state_for(unique_home("daemon-log-unknown-kind")),
    )
    .await;
    let daemon_body: Value = serde_json::from_slice(response_buffer(&daemon)).expect("json");
    assert_eq!(daemon.status_code, 404);
    assert_eq!(daemon_body["error"], "not_found");

    let home = unique_home("server-log-unknown-kind");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8912),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");

    let server = route_request(
        &get(&format!(
            "/v1/servers/{}/logs/combined",
            outcome.spec.short_ref
        )),
        &state_for(home),
    )
    .await;
    let server_body: Value = serde_json::from_slice(response_buffer(&server)).expect("json");
    assert_eq!(server.status_code, 404);
    assert_eq!(server_body["error"], "not_found");
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

#[tokio::test]
async fn openai_chat_completions_requires_daemon_auth_when_token_enabled() {
    let request = post("/v1/chat/completions", b"{}");
    let state = state_with_token(unique_home("openai-auth"), "secret");
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 401);
    assert_eq!(body["error"], "unauthorized");
}

#[tokio::test]
async fn openai_chat_completions_validates_basic_request_shape() {
    for body in [
        br#"{"messages":[{"role":"user","content":"Hello"}]}"#.as_slice(),
        br#"{"model":"abc","messages":[]}"#.as_slice(),
        br#"{"model":"abc","messages":"bad"}"#.as_slice(),
        br#"{"model":"abc","messages":[{"role":"tool","content":"Hello"}]}"#.as_slice(),
        br#"{"model":"abc","messages":[{"role":"user","content":[{"type":"text","text":"Hello"}]}]}"#.as_slice(),
        br#"{"model":"abc","messages":[{"role":"user","content":"Hello"}],"stream":"yes"}"#.as_slice(),
        br#"{"model":"abc","messages":[{"role":"user","content":"Hello"}],"max_tokens":1.5}"#.as_slice(),
        br#"{"model":"abc","messages":[{"role":"user","content":"Hello"}],"temperature":"cold"}"#.as_slice(),
    ] {
        let request = post("/v1/chat/completions", body);
        let state = state_for(unique_home("openai-invalid-request"));
        let response = route_request(&request, &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

        assert_eq!(response.status_code, 400);
        assert_eq!(body["error"], "bad_request");
    }
}

#[tokio::test]
async fn openai_chat_completions_model_uses_server_ref_not_provider_model_name() {
    let request = post(
        "/v1/chat/completions",
        br#"{"model":"gpt-4.1-mini","messages":[{"role":"user","content":"Hello"}]}"#,
    );
    let state = state_for(unique_home("openai-provider-model-name"));
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 404);
    assert_eq!(body["error"], "not_found");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("Tentgent server ref"));
}

#[tokio::test]
async fn openai_chat_completions_stopped_server_returns_409() {
    let home = unique_home("openai-stopped-server");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8904),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("server spec");
    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","messages":[{{"role":"user","content":"Hello"}}]}}"#,
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
async fn openai_chat_completions_ambiguous_model_prefix_returns_409() {
    let home = unique_home("openai-ambiguous-model");
    write_cloud_server_spec(
        &home,
        "abcdef1111111111111111111111111111111111111111111111111111111111",
    );
    write_cloud_server_spec(
        &home,
        "abcdef2222222222222222222222222222222222222222222222222222222222",
    );

    let request = post(
        "/v1/chat/completions",
        br#"{"model":"abcdef","messages":[{"role":"user","content":"Hello"}]}"#,
    );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 409);
    assert_eq!(body["error"], "ambiguous_ref");
}

#[tokio::test]
async fn openai_chat_completions_maps_target_text_response() {
    let (port, received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"text":"hello","stream":false}"#,
        true,
    )
    .await;
    let home = unique_home("openai-chat-json");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","messages":[{{"role":"user","content":"Hello"}}],"stream":false,"max_tokens":16,"temperature":0.1,"unsupported":"ignored"}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let state = state_for(home.clone());
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let proxied_body: Value =
        serde_json::from_slice(&received_body.await.expect("mock received body"))
            .expect("proxied json");

    assert_eq!(response.status_code, 200);
    assert!(body["id"].as_str().expect("id").starts_with("chatcmpl-"));
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["model"], expanded_server_ref(&home, &server_ref));
    assert_eq!(body["choices"][0]["message"]["role"], "assistant");
    assert_eq!(body["choices"][0]["message"]["content"], "hello");
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
    assert_eq!(proxied_body["messages"][0]["content"], "Hello");
    assert_eq!(proxied_body["max_tokens"], 16);
    assert_eq!(proxied_body["temperature"], 0.1);
    assert!(proxied_body.get("unsupported").is_none());
}

#[tokio::test]
async fn openai_chat_completions_preserves_target_error_json() {
    let (port, _received_body) = spawn_mock_chat_server(
        501,
        "application/json; charset=utf-8",
        br#"{"error":"stream_not_implemented","message":"nope"}"#,
        true,
    )
    .await;
    let home = unique_home("openai-target-error");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","messages":[{{"role":"user","content":"Hello"}}],"stream":false}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let response = route_request(&request, &state_for(home)).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 501);
    assert_eq!(body["error"], "stream_not_implemented");
}

#[tokio::test]
async fn openai_chat_completions_rejects_unmappable_target_json() {
    let (port, _received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"message":{"role":"assistant","content":"hello"}}"#,
        true,
    )
    .await;
    let home = unique_home("openai-unmappable-json");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","messages":[{{"role":"user","content":"Hello"}}]}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let response = route_request(&request, &state_for(home)).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 502);
    assert_eq!(body["error"], "server_proxy_failed");
}

#[tokio::test]
async fn openai_chat_completions_maps_tentgent_sse_to_openai_chunks() {
    let sse = b"event: delta\ndata: {\"delta\":\"hi\"}\n\nevent: done\ndata: {\"finish_reason\":\"stop\"}\n\n";
    let (port, _received_body) =
        spawn_mock_chat_server(200, "text/event-stream; charset=utf-8", sse, false).await;
    let home = unique_home("openai-chat-sse");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","messages":[{{"role":"user","content":"Hello"}}],"stream":true}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let response = route_request(&request, &state_for(home)).await;
    let content_type = response.content_type.clone();
    let cache_control = response.cache_control.clone();
    let body = String::from_utf8(collect_response_body(response).await).expect("utf8");

    assert_eq!(content_type, "text/event-stream; charset=utf-8");
    assert_eq!(cache_control.as_deref(), Some("no-cache"));
    assert!(body.contains(r#""object":"chat.completion.chunk""#));
    assert!(body.contains(r#""content":"hi""#));
    assert!(body.contains(r#""finish_reason":"stop""#));
    assert!(body.ends_with("data: [DONE]\n\n"));
}

#[tokio::test]
async fn openai_chat_completions_transport_failure_returns_502() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    let home = unique_home("openai-transport-failure");
    let server_ref = create_running_cloud_server(&home, port);
    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","messages":[{{"role":"user","content":"Hello"}}]}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let response = route_request(&request, &state_for(home)).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 502);
    assert_eq!(body["error"], "server_proxy_failed");
}

fn get(path: &str) -> HttpRequest {
    let (path, query_params) = test_split_target(path);
    HttpRequest {
        method: "GET".to_string(),
        path,
        query_params,
        version: "HTTP/1.1".to_string(),
        headers: Vec::new(),
        body: Vec::new(),
        parse_error: None,
    }
}

fn get_with_header(path: &str, name: &str, value: &str) -> HttpRequest {
    let mut request = get(path);
    request.headers.push((name.to_string(), value.to_string()));
    request
}

fn post(path: &str, body: &[u8]) -> HttpRequest {
    let (path, query_params) = test_split_target(path);
    HttpRequest {
        method: "POST".to_string(),
        path,
        query_params,
        version: "HTTP/1.1".to_string(),
        headers: Vec::new(),
        body: body.to_vec(),
        parse_error: None,
    }
}

fn test_split_target(target: &str) -> (String, Vec<(String, String)>) {
    let Some((path, query)) = target.split_once('?') else {
        return (target.to_string(), Vec::new());
    };

    let query_params = query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (name, value) = part.split_once('=').unwrap_or((part, ""));
            (name.to_string(), value.to_string())
        })
        .collect();

    (path.to_string(), query_params)
}

fn response_buffer(response: &HttpResponse) -> &[u8] {
    match &response.body {
        HttpBody::Buffered(body) => body,
        HttpBody::Proxy(_) => panic!("expected buffered response"),
        HttpBody::Stream(_) => panic!("expected buffered response"),
    }
}

fn response_header<'a>(response: &'a HttpResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
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
        HttpBody::Stream(mut chunks) => {
            let mut body = Vec::new();
            while let Some(chunk) = chunks.recv().await {
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

fn expanded_server_ref(home: &Path, reference: &str) -> String {
    ServerManager::open_readonly(Some(home))
        .expect("server manager")
        .inspect(reference)
        .expect("server inspect")
        .spec
        .server_ref
}

fn write_cloud_server_spec(home: &Path, server_ref: &str) {
    let server_dir = home.join("servers").join(server_ref);
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(
        server_dir.join("server.toml"),
        format!(
            r#"server_ref = "{server_ref}"
short_ref = "{}"
runtime_kind = "cloud"
provider = "openai"
provider_model = "gpt-4.1-mini"
host = "127.0.0.1"
port = 8000
lazy_load = false
created_at = "2026-05-01T00:00:00Z"
"#,
            &server_ref[..12]
        ),
    )
    .expect("server spec");
}

fn write_session_fixture(
    home: &Path,
    session_ref: &str,
    title: &str,
    created_at: &str,
    updated_at: &str,
    message_count: usize,
    messages: Option<&[String]>,
) {
    let session_dir = home.join("sessions").join(session_ref);
    fs::create_dir_all(&session_dir).expect("session dir");
    fs::write(
        session_dir.join("session.toml"),
        format!(
            r#"schema = "tentgent.session.v1"
session_ref = "{session_ref}"
short_ref = "{}"
title = "{title}"
created_at = "{created_at}"
updated_at = "{updated_at}"
message_count = {message_count}
tags = []
"#,
            &session_ref[..12]
        ),
    )
    .expect("session metadata");
    if let Some(messages) = messages {
        fs::write(
            session_dir.join("messages.jsonl"),
            messages.join("\n") + "\n",
        )
        .expect("session messages");
    }
}

fn session_message(role: &str, content: &str) -> String {
    format!(
        r#"{{"schema":"tentgent.session.message.v1","role":"{role}","content":"{content}","created_at":"2026-05-01T00:00:00Z"}}"#
    )
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

fn state_with_token(home: PathBuf, token: &str) -> DaemonHttpState {
    DaemonHttpState::with_security(
        inspection(home),
        DaemonSecurityConfig::from_token_value(Some(token)),
    )
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
