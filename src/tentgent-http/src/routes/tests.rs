use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use tentgent_core::{
    daemon::{DaemonInspection, DaemonProcessMetadata},
    dataset::DatasetManager,
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
    http::{
        find_header_end, reason_phrase, HttpAfterWriteAction, HttpBody, HttpRequest, HttpResponse,
    },
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
async fn auth_routes_return_local_status_without_secret_values() {
    const SENTINEL: &str = "tentgent-slice17-secret-sentinel";
    let previous_openai = std::env::var("OPENAI_API_KEY").ok();
    let previous_hf = std::env::var("HF_TOKEN").ok();
    let previous_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::set_var("OPENAI_API_KEY", SENTINEL);
    std::env::set_var("HF_TOKEN", format!("{SENTINEL}-hf"));
    std::env::set_var("ANTHROPIC_API_KEY", format!("{SENTINEL}-anthropic"));

    let state = state_for(unique_home("auth-status-local"));
    let response = route_request(&get("/v1/auth"), &state).await;
    let body_text = String::from_utf8_lossy(response_buffer(&response)).to_string();
    let body: Value = serde_json::from_str(&body_text).expect("json");

    assert_eq!(response.status_code, 200);
    assert!(!body_text.contains(SENTINEL));
    assert_eq!(body["providers"].as_array().expect("providers").len(), 3);
    let openai = body["providers"]
        .as_array()
        .expect("providers")
        .iter()
        .find(|provider| provider["provider"] == "openai")
        .expect("openai");
    assert_eq!(openai["env_present"], true);
    assert_eq!(openai["effective_source"], "env");
    assert_eq!(openai["validation"]["state"], "not_checked");

    let one = route_request(&get("/v1/auth/openai"), &state).await;
    let one_body: Value = serde_json::from_slice(response_buffer(&one)).expect("json");
    assert_eq!(one.status_code, 200);
    assert_eq!(one_body["provider"]["provider"], "openai");
    assert_eq!(one_body["provider"]["validation"]["state"], "not_checked");

    let alias = route_request(&get("/v1/auth/huggingface"), &state).await;
    let alias_body: Value = serde_json::from_slice(response_buffer(&alias)).expect("json");
    assert_eq!(alias.status_code, 400);
    assert_eq!(alias_body["error"], "bad_request");

    let case_variant = route_request(&get("/v1/auth/OpenAI"), &state).await;
    assert_eq!(case_variant.status_code, 400);

    restore_env("OPENAI_API_KEY", previous_openai);
    restore_env("HF_TOKEN", previous_hf);
    restore_env("ANTHROPIC_API_KEY", previous_anthropic);
}

#[tokio::test]
async fn doctor_route_returns_observational_report() {
    const SENTINEL: &str = "tentgent-doctor-secret-sentinel";
    let previous = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::set_var("ANTHROPIC_API_KEY", SENTINEL);

    let home = unique_home("doctor-route");
    let state = state_with_token(home.clone(), "secret");
    let unauthorized = route_request(&get("/v1/doctor"), &state).await;
    assert_eq!(unauthorized.status_code, 401);

    let response = route_request(
        &get_with_header("/v1/doctor", "Authorization", "Bearer secret"),
        &state,
    )
    .await;
    let body_text = String::from_utf8_lossy(response_buffer(&response)).to_string();
    let body: Value = serde_json::from_str(&body_text).expect("json");

    assert_eq!(response.status_code, 200);
    assert!(!body_text.contains(SENTINEL));
    assert!(matches!(
        body["status"].as_str().expect("status"),
        "pass" | "warn" | "fail"
    ));
    assert!(body["summary"]["pass"].as_u64().is_some());
    assert!(body["summary"]["warn"].as_u64().is_some());
    assert!(body["summary"]["fail"].as_u64().is_some());
    assert!(body["summary"]["skipped"].as_u64().is_some());
    assert!(!body["checks"].as_array().expect("checks").is_empty());
    let runtime_home = body["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["name"] == "runtime home")
        .expect("runtime home");
    assert!(runtime_home["detail"]
        .as_str()
        .expect("detail")
        .contains(&home.display().to_string()));

    restore_env("ANTHROPIC_API_KEY", previous);
}

#[tokio::test]
async fn daemon_shutdown_requires_token_and_returns_after_write_action() {
    let no_token_state = state_for(unique_home("shutdown-no-token"));
    let no_token = route_request(&post("/v1/daemon/shutdown", b"{}"), &no_token_state).await;
    let no_token_body: Value = serde_json::from_slice(response_buffer(&no_token)).expect("json");
    assert_eq!(no_token.status_code, 409);
    assert_eq!(no_token_body["error"], "daemon_token_required");

    let state = state_with_token(unique_home("shutdown-token"), "secret");
    let unauthorized = route_request(&post("/v1/daemon/shutdown", b"{}"), &state).await;
    assert_eq!(unauthorized.status_code, 401);

    for body in [
        b"null".as_slice(),
        b"[]".as_slice(),
        br#"{"reason":"test"}"#.as_slice(),
    ] {
        let bad = route_request(
            &post_with_header(
                "/v1/daemon/shutdown",
                body,
                "Authorization",
                "Bearer secret",
            ),
            &state,
        )
        .await;
        let bad_body: Value = serde_json::from_slice(response_buffer(&bad)).expect("json");
        assert_eq!(bad.status_code, 400);
        assert_eq!(bad_body["error"], "bad_request");
    }

    let accepted = route_request(
        &post_with_header(
            "/v1/daemon/shutdown",
            b"{}",
            "Authorization",
            "Bearer secret",
        ),
        &state,
    )
    .await;
    let accepted_body: Value = serde_json::from_slice(response_buffer(&accepted)).expect("json");
    assert_eq!(accepted.status_code, 202);
    assert_eq!(accepted_body["shutdown"]["accepted"], true);
    assert_eq!(accepted_body["shutdown"]["pid"], 1234);
    assert_eq!(
        accepted.after_write,
        Some(HttpAfterWriteAction::RequestDaemonShutdown)
    );

    let empty = route_request(
        &post_with_header("/v1/daemon/shutdown", b"", "Authorization", "Bearer secret"),
        &state,
    )
    .await;
    assert_eq!(empty.status_code, 202);

    let logs = route_request(&get("/v1/daemon/logs"), &state_for(unique_home("logs"))).await;
    assert_eq!(logs.status_code, 200);
    let logs_post = route_request(&post("/v1/daemon/logs", b""), &no_token_state).await;
    assert_eq!(logs_post.status_code, 405);
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
async fn model_inspect_and_delete_work_by_full_ref_and_prefix() {
    let home = unique_home("model-inspect-delete");
    let model_ref = "111111111111000000000000";
    write_model_fixture(&home, model_ref);
    let state = state_for(home.clone());

    let list = route_request(&get("/v1/models"), &state).await;
    let list_body: Value = serde_json::from_slice(response_buffer(&list)).expect("json");
    assert_eq!(list.status_code, 200);
    assert_eq!(list_body["models"].as_array().expect("models").len(), 1);
    assert!(list_body["models"][0].get("manifest_path").is_none());

    let inspect = route_request(&get(&format!("/v1/models/{model_ref}")), &state).await;
    let inspect_body: Value = serde_json::from_slice(response_buffer(&inspect)).expect("json");
    assert_eq!(inspect.status_code, 200);
    assert_eq!(inspect_body["model"]["model_ref"], model_ref);
    assert_eq!(
        inspect_body["model"]["manifest_path"],
        path_string(&home.join(format!("models/store/{model_ref}/manifest.json")))
    );
    assert_eq!(
        inspect_body["model"]["variant_source_path"],
        path_string(&home.join(format!("models/store/{model_ref}/variants/mlx/source")))
    );

    let remove = route_request(&delete("/v1/models/111111111111", b""), &state).await;
    let remove_body: Value = serde_json::from_slice(response_buffer(&remove)).expect("json");
    assert_eq!(remove.status_code, 200);
    assert_eq!(remove_body["removed"]["kind"], "model");
    assert_eq!(remove_body["removed"]["model_ref"], model_ref);
    assert_eq!(remove_body["model"]["model_ref"], model_ref);
    assert!(!home.join(format!("models/store/{model_ref}")).exists());

    let missing = route_request(&get(&format!("/v1/models/{model_ref}")), &state).await;
    let missing_body: Value = serde_json::from_slice(response_buffer(&missing)).expect("json");
    assert_eq!(missing.status_code, 404);
    assert_eq!(missing_body["error"], "not_found");
}

#[tokio::test]
async fn model_delete_rejects_body_auth_ambiguous_path_and_in_use_cases() {
    let home = unique_home("model-delete-errors");
    write_model_fixture(&home, "aaaaaaaaaaaa000000000000");
    write_model_fixture(&home, "aaaaaaaaaaab000000000000");
    let state = state_for(home.clone());

    let ambiguous = route_request(&delete("/v1/models/aaaa", b""), &state).await;
    let ambiguous_body: Value = serde_json::from_slice(response_buffer(&ambiguous)).expect("json");
    assert_eq!(ambiguous.status_code, 409);
    assert_eq!(ambiguous_body["error"], "ambiguous_ref");

    let body = route_request(&delete("/v1/models/aaaaaaaaaaaa", b"{}"), &state).await;
    let body_json: Value = serde_json::from_slice(response_buffer(&body)).expect("json");
    assert_eq!(body.status_code, 400);
    assert_eq!(body_json["error"], "bad_request");

    let path_like = route_request(&delete("/v1/models/../aaaaaaaaaaaa", b""), &state).await;
    assert_eq!(path_like.status_code, 404);
    assert!(home.join("aaaaaaaaaaaa").exists() == false);

    let token_state = state_with_token(home.clone(), "secret");
    let unauthorized = route_request(&delete("/v1/models/aaaaaaaaaaaa", b""), &token_state).await;
    assert_eq!(unauthorized.status_code, 401);

    write_local_server_spec_for_model(&home, "model-server-ref", "aaaaaaaaaaaa000000000000");
    let in_use = route_request(&delete("/v1/models/aaaaaaaaaaaa", b""), &state).await;
    let in_use_body: Value = serde_json::from_slice(response_buffer(&in_use)).expect("json");
    assert_eq!(in_use.status_code, 409);
    assert_eq!(in_use_body["error"], "in_use");
    assert!(in_use_body["message"]
        .as_str()
        .expect("message")
        .contains("server spec"));
}

#[tokio::test]
async fn model_import_works_repeats_and_validates_inputs() {
    let home = unique_home("model-import");
    let source = write_model_import_source(&home);
    let state = state_for(home.clone());

    let response = route_request(
        &post(
            "/v1/models/import",
            format!(r#"{{"path":"{}"}}"#, path_string(&source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let model_ref = body["model"]["model_ref"].as_str().expect("model_ref");
    assert_eq!(response.status_code, 200);
    assert!(body.get("job").is_none());
    assert_eq!(body["mutation"]["kind"], "import");
    assert_eq!(body["mutation"]["deduplicated"], false);
    assert_eq!(body["model"]["source_path"], path_string(&source));

    let repeated = route_request(
        &post(
            "/v1/models/import",
            format!(r#"{{"path":"{}"}}"#, path_string(&source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let repeated_body: Value = serde_json::from_slice(response_buffer(&repeated)).expect("json");
    assert_eq!(repeated.status_code, 200);
    assert_eq!(repeated_body["model"]["model_ref"], model_ref);
    assert_eq!(repeated_body["mutation"]["deduplicated"], true);

    for (payload, expected_status, expected_error) in [
        (
            r#"{"path":"relative/model"}"#.to_string(),
            400,
            "bad_request",
        ),
        (r#"{"path":" "}"#.to_string(), 400, "bad_request"),
        (
            format!(r#"{{"path":"{}"}}"#, path_string(&home.join("missing"))),
            404,
            "path_not_found",
        ),
        (
            format!(r#"{{"path":"{}","force":true}}"#, path_string(&source)),
            400,
            "bad_request",
        ),
    ] {
        let response = route_request(&post("/v1/models/import", payload.as_bytes()), &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, expected_status);
        assert_eq!(body["error"], expected_error);
    }

    let unsupported = home.join("fixtures/unsupported-model");
    fs::create_dir_all(&unsupported).expect("unsupported dir");
    fs::write(unsupported.join("README.md"), "no model").expect("unsupported file");
    let response = route_request(
        &post(
            "/v1/models/import",
            format!(r#"{{"path":"{}"}}"#, path_string(&unsupported)).as_bytes(),
        ),
        &state,
    )
    .await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    assert_eq!(response.status_code, 400);
    assert_eq!(body["error"], "unsupported_layout");
}

#[tokio::test]
async fn async_import_job_returns_accepted_and_persists_job_record() {
    let home = unique_home("model-import-job");
    let source = write_model_import_source(&home);
    let state = state_for(home.clone());

    let response = route_request(
        &post(
            "/v1/models/import/jobs",
            format!(r#"{{"path":"{}"}}"#, path_string(&source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let job_id = body["job"]["job_id"].as_str().expect("job id").to_string();

    assert_eq!(response.status_code, 202);
    assert_eq!(body["job"]["kind"], "model_import");
    assert_eq!(body["job"]["target_section"], "models");
    assert_eq!(body["job"]["cancellable"], false);

    for _ in 0..20 {
        let inspect = route_request(&get(&format!("/v1/jobs/{job_id}")), &state).await;
        let inspect_body: Value = serde_json::from_slice(response_buffer(&inspect)).expect("json");
        if inspect_body["job"]["status"] == "succeeded" {
            assert_eq!(inspect_body["job"]["refresh_targets"][0], "models");
            assert!(inspect_body["job"]["artifact_path"]
                .as_str()
                .expect("artifact path")
                .contains("/models/store/"));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("model import job did not finish");
}

#[tokio::test]
async fn async_job_validation_happens_before_job_creation() {
    let state = state_for(unique_home("job-validation"));
    let response = route_request(&post("/v1/models/pull/jobs", br#"{"repo_id":""}"#), &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    assert_eq!(response.status_code, 400);
    assert_eq!(body["error"], "bad_request");

    let jobs = route_request(&get("/v1/jobs"), &state).await;
    let jobs_body: Value = serde_json::from_slice(response_buffer(&jobs)).expect("json");
    assert_eq!(jobs_body["jobs"].as_array().expect("jobs").len(), 0);
}

#[tokio::test]
async fn jobs_routes_follow_daemon_auth() {
    let state = state_with_token(unique_home("jobs-auth"), "secret");
    let unauthorized = route_request(&get("/v1/jobs"), &state).await;
    assert_eq!(unauthorized.status_code, 401);

    let authorized = route_request(
        &get_with_header("/v1/jobs", "Authorization", "Bearer secret"),
        &state,
    )
    .await;
    assert_eq!(authorized.status_code, 200);
}

#[tokio::test]
async fn pull_routes_validate_repo_id_and_revision_without_network() {
    let state = state_for(unique_home("pull-validation"));
    for (path, payload) in [
        ("/v1/models/pull", br#"{"repo_id":""}"#.as_slice()),
        (
            "/v1/models/pull",
            br#"{"repo_id":"https://huggingface.co/owner/name"}"#.as_slice(),
        ),
        (
            "/v1/models/pull",
            br#"{"repo_id":"owner/name/tree/main"}"#.as_slice(),
        ),
        (
            "/v1/adapters/pull",
            br#"{"repo_id":"owner/name","revision":" "}"#.as_slice(),
        ),
        (
            "/v1/adapters/pull",
            br#"{"repo_id":"owner/name","unknown":true}"#.as_slice(),
        ),
    ] {
        let response = route_request(&post(path, payload), &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400);
        assert_eq!(body["error"], "bad_request");
    }
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
async fn adapter_inspect_and_delete_work_and_in_use_is_protected() {
    let home = unique_home("adapter-inspect-delete");
    let adapter_ref = "222222222222000000000000";
    write_adapter_fixture(&home, adapter_ref);
    let state = state_for(home.clone());

    let inspect = route_request(&get("/v1/adapters/222222222222"), &state).await;
    let inspect_body: Value = serde_json::from_slice(response_buffer(&inspect)).expect("json");
    assert_eq!(inspect.status_code, 200);
    assert_eq!(inspect_body["adapter"]["adapter_ref"], adapter_ref);
    assert_eq!(
        inspect_body["adapter"]["managed_source_path"],
        path_string(&home.join(format!("adapters/store/{adapter_ref}/source")))
    );

    write_cloud_server_spec_with_adapter(&home, "adapter-server-ref", adapter_ref);
    let in_use = route_request(&delete("/v1/adapters/222222222222", b""), &state).await;
    let in_use_body: Value = serde_json::from_slice(response_buffer(&in_use)).expect("json");
    assert_eq!(in_use.status_code, 409);
    assert_eq!(in_use_body["error"], "in_use");

    fs::remove_dir_all(home.join("servers/adapter-server-ref")).expect("remove server");
    let remove = route_request(&delete("/v1/adapters/222222222222", b""), &state).await;
    let remove_body: Value = serde_json::from_slice(response_buffer(&remove)).expect("json");
    assert_eq!(remove.status_code, 200);
    assert_eq!(remove_body["removed"]["kind"], "adapter");
    assert_eq!(remove_body["removed"]["adapter_ref"], adapter_ref);
    assert_eq!(remove_body["adapter"]["adapter_ref"], adapter_ref);
    assert!(!home.join(format!("adapters/store/{adapter_ref}")).exists());
}

#[tokio::test]
async fn adapter_import_and_bind_work_with_daemon_home_model_lookup() {
    let home = unique_home("adapter-import-bind");
    let model_ref = "999999999999000000000000";
    write_model_fixture(&home, model_ref);
    let source = write_adapter_import_source(&home);
    let state = state_for(home.clone());

    let import = route_request(
        &post(
            "/v1/adapters/import",
            format!(r#"{{"path":"{}"}}"#, path_string(&source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let import_body: Value = serde_json::from_slice(response_buffer(&import)).expect("json");
    let adapter_ref = import_body["adapter"]["adapter_ref"]
        .as_str()
        .expect("adapter_ref")
        .to_string();
    assert_eq!(import.status_code, 200);
    assert_eq!(import_body["mutation"]["kind"], "import");
    assert_eq!(import_body["adapter"]["base_model_ref"], Value::Null);

    let bind = route_request(
        &post(
            &format!("/v1/adapters/{}/bind", &adapter_ref[..12]),
            format!(r#"{{"base_model_ref":"{}"}}"#, &model_ref[..12]).as_bytes(),
        ),
        &state,
    )
    .await;
    let bind_body: Value = serde_json::from_slice(response_buffer(&bind)).expect("json");
    assert_eq!(bind.status_code, 200);
    assert_eq!(bind_body["mutation"]["kind"], "bind");
    assert_eq!(bind_body["mutation"]["base_model_ref"], model_ref);
    assert_eq!(bind_body["adapter"]["base_model_ref"], model_ref);
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
async fn dataset_inspect_and_delete_work() {
    let home = unique_home("dataset-inspect-delete");
    let dataset_ref = "333333333333000000000000";
    write_dataset_fixture(&home, dataset_ref);
    let state = state_for(home.clone());

    let inspect = route_request(&get("/v1/datasets/333333333333"), &state).await;
    let inspect_body: Value = serde_json::from_slice(response_buffer(&inspect)).expect("json");
    assert_eq!(inspect.status_code, 200);
    assert_eq!(inspect_body["dataset"]["dataset_ref"], dataset_ref);
    assert_eq!(inspect_body["dataset"]["tuning_ready"], true);
    assert_eq!(inspect_body["dataset"]["splits"]["train"], "train.jsonl");
    assert_eq!(
        inspect_body["dataset"]["managed_source_path"],
        path_string(&home.join(format!("datasets/store/{dataset_ref}/source")))
    );

    let remove = route_request(&delete("/v1/datasets/333333333333", b""), &state).await;
    let remove_body: Value = serde_json::from_slice(response_buffer(&remove)).expect("json");
    assert_eq!(remove.status_code, 200);
    assert_eq!(remove_body["removed"]["kind"], "dataset");
    assert_eq!(remove_body["removed"]["dataset_ref"], dataset_ref);
    assert_eq!(remove_body["dataset"]["dataset_ref"], dataset_ref);

    let missing = route_request(&delete("/v1/datasets/333333333333", b""), &state).await;
    let missing_body: Value = serde_json::from_slice(response_buffer(&missing)).expect("json");
    assert_eq!(missing.status_code, 404);
    assert_eq!(missing_body["error"], "not_found");
}

#[tokio::test]
async fn dataset_import_works_and_repeats() {
    let home = unique_home("dataset-import");
    let source = write_dataset_import_source(&home);
    let state = state_for(home.clone());

    let first = route_request(
        &post(
            "/v1/datasets/import",
            format!(r#"{{"path":"{}"}}"#, path_string(&source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let first_body: Value = serde_json::from_slice(response_buffer(&first)).expect("json");
    assert_eq!(first.status_code, 200);
    assert_eq!(first_body["mutation"]["kind"], "import");
    assert_eq!(first_body["mutation"]["deduplicated"], false);
    assert_eq!(first_body["dataset"]["tuning_ready"], true);

    let second = route_request(
        &post(
            "/v1/datasets/import",
            format!(r#"{{"path":"{}"}}"#, path_string(&source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let second_body: Value = serde_json::from_slice(response_buffer(&second)).expect("json");
    assert_eq!(second.status_code, 200);
    assert_eq!(
        second_body["dataset"]["dataset_ref"],
        first_body["dataset"]["dataset_ref"]
    );
    assert_eq!(second_body["mutation"]["deduplicated"], true);
}

#[tokio::test]
async fn dataset_validate_path_and_managed_refs_work() {
    let home = unique_home("dataset-validate");
    let source = write_dataset_import_source(&home);
    let state = state_for(home.clone());

    let path_response = route_request(
        &post(
            "/v1/datasets/validate",
            format!(r#"{{"path":"{}"}}"#, path_string(&source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let path_body: Value = serde_json::from_slice(response_buffer(&path_response)).expect("json");
    assert_eq!(path_response.status_code, 200);
    assert_eq!(path_body["valid"], true);
    assert_eq!(path_body["source"]["kind"], "path");
    assert_eq!(path_body["target"], "directory");
    assert_eq!(path_body["records"], 1);
    assert_eq!(path_body["errors_count"], 0);
    assert_eq!(path_body["splits"][0]["name"], "train");

    let manager = DatasetManager::new_with_home(Some(&home)).expect("dataset manager");
    let import = manager.add_path(&source).expect("import dataset");
    let managed = route_request(
        &post(
            "/v1/datasets/validate",
            format!(r#"{{"dataset_ref":"{}"}}"#, import.metadata.short_ref).as_bytes(),
        ),
        &state,
    )
    .await;
    let managed_body: Value = serde_json::from_slice(response_buffer(&managed)).expect("json");
    assert_eq!(managed.status_code, 200);
    assert_eq!(managed_body["valid"], true);
    assert_eq!(managed_body["source"]["kind"], "dataset");
    assert_eq!(
        managed_body["source"]["dataset_ref"],
        import.metadata.dataset_ref
    );
    assert_eq!(
        managed_body["source"]["short_ref"],
        import.metadata.short_ref
    );
}

#[tokio::test]
async fn dataset_validate_invalid_schema_is_200_but_bad_request_shapes_are_400() {
    let home = unique_home("dataset-validate-errors");
    let invalid = write_invalid_dataset_source(&home);
    let state = state_for(home.clone());

    let invalid_response = route_request(
        &post(
            "/v1/datasets/validate",
            format!(r#"{{"path":"{}"}}"#, path_string(&invalid)).as_bytes(),
        ),
        &state,
    )
    .await;
    let invalid_body: Value =
        serde_json::from_slice(response_buffer(&invalid_response)).expect("json");
    assert_eq!(invalid_response.status_code, 200);
    assert_eq!(invalid_body["valid"], false);
    assert_eq!(invalid_body["errors_count"], 1);
    assert_eq!(invalid_body["errors"][0]["line"], 1);

    for payload in [
        r#"{}"#.to_string(),
        format!(
            r#"{{"path":"{}","dataset_ref":"abc"}}"#,
            path_string(&invalid)
        ),
        r#"{"path":"relative/dataset"}"#.to_string(),
        r#"{"dataset_ref":"../dataset"}"#.to_string(),
        r#"{"path":" ","dataset_ref":" "}"#.to_string(),
        format!(r#"{{"path":"{}","unknown":true}}"#, path_string(&invalid)),
    ] {
        let response =
            route_request(&post("/v1/datasets/validate", payload.as_bytes()), &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400, "{payload}");
        assert_eq!(body["error"], "bad_request");
    }

    let missing_path = route_request(
        &post(
            "/v1/datasets/validate",
            format!(r#"{{"path":"{}"}}"#, path_string(&home.join("missing"))).as_bytes(),
        ),
        &state,
    )
    .await;
    let missing_path_body: Value =
        serde_json::from_slice(response_buffer(&missing_path)).expect("json");
    assert_eq!(missing_path.status_code, 404);
    assert_eq!(missing_path_body["error"], "path_not_found");

    write_dataset_fixture(&home, "aaaaaaaaaaaa000000000000");
    write_dataset_fixture(&home, "aaaaaaaaaaab000000000000");
    let missing_ref = route_request(
        &post("/v1/datasets/validate", br#"{"dataset_ref":"missing"}"#),
        &state,
    )
    .await;
    let missing_ref_body: Value =
        serde_json::from_slice(response_buffer(&missing_ref)).expect("json");
    assert_eq!(missing_ref.status_code, 404);
    assert_eq!(missing_ref_body["error"], "not_found");

    let ambiguous = route_request(
        &post("/v1/datasets/validate", br#"{"dataset_ref":"aaaa"}"#),
        &state,
    )
    .await;
    let ambiguous_body: Value = serde_json::from_slice(response_buffer(&ambiguous)).expect("json");
    assert_eq!(ambiguous.status_code, 409);
    assert_eq!(ambiguous_body["error"], "ambiguous_ref");
}

#[tokio::test]
async fn dataset_template_returns_content_defaults_and_rejects_unknown_fields() {
    let state = state_for(unique_home("dataset-template"));

    let response = route_request(
        &post(
            "/v1/datasets/template",
            br#"{"task":"support","language":"zh-TW"}"#,
        ),
        &state,
    )
    .await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    assert_eq!(response.status_code, 200);
    assert_eq!(body["template_version"], "tentgent.dataset.synth.v1");
    assert_eq!(body["task"], "support");
    assert_eq!(body["language"], "zh-TW");
    assert!(body["content"]
        .as_str()
        .expect("content")
        .contains("tentgent.chat.v1"));

    let defaults = route_request(
        &post("/v1/datasets/template", br#"{"task":" ","language":" "}"#),
        &state,
    )
    .await;
    let defaults_body: Value = serde_json::from_slice(response_buffer(&defaults)).expect("json");
    assert_eq!(defaults.status_code, 200);
    assert_eq!(defaults_body["task"], "chat");
    assert_eq!(defaults_body["language"], "en");

    let bad = route_request(
        &post("/v1/datasets/template", br#"{"output_path":"/tmp/x"}"#),
        &state,
    )
    .await;
    let bad_body: Value = serde_json::from_slice(response_buffer(&bad)).expect("json");
    assert_eq!(bad.status_code, 400);
    assert_eq!(bad_body["error"], "bad_request");
}

#[tokio::test]
async fn dataset_export_writes_files_and_maps_output_errors() {
    let home = unique_home("dataset-export");
    let source = write_dataset_import_source(&home);
    let manager = DatasetManager::new_with_home(Some(&home)).expect("dataset manager");
    let import = manager.add_path(&source).expect("import dataset");
    let state = state_for(home.clone());

    let output = home.join("exports/new");
    let response = route_request(
        &post(
            &format!("/v1/datasets/{}/export", import.metadata.short_ref),
            format!(r#"{{"output_path":"{}"}}"#, path_string(&output)).as_bytes(),
        ),
        &state,
    )
    .await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    assert_eq!(response.status_code, 200);
    assert_eq!(body["dataset"]["dataset_ref"], import.metadata.dataset_ref);
    assert_eq!(body["export"]["output_path"], path_string(&output));
    assert_eq!(body["export"]["file_count"], 1);
    assert!(output.join("train.jsonl").exists());

    let empty = home.join("exports/empty");
    fs::create_dir_all(&empty).expect("empty export dir");
    let empty_response = route_request(
        &post(
            &format!("/v1/datasets/{}/export", import.metadata.dataset_ref),
            format!(r#"{{"output_path":"{}"}}"#, path_string(&empty)).as_bytes(),
        ),
        &state,
    )
    .await;
    assert_eq!(empty_response.status_code, 200);

    let non_empty = home.join("exports/non-empty");
    fs::create_dir_all(&non_empty).expect("non-empty dir");
    fs::write(non_empty.join("existing.txt"), "existing").expect("existing file");
    let exists = route_request(
        &post(
            &format!("/v1/datasets/{}/export", import.metadata.short_ref),
            format!(r#"{{"output_path":"{}"}}"#, path_string(&non_empty)).as_bytes(),
        ),
        &state,
    )
    .await;
    let exists_body: Value = serde_json::from_slice(response_buffer(&exists)).expect("json");
    assert_eq!(exists.status_code, 409);
    assert_eq!(exists_body["error"], "output_exists");

    let relative = route_request(
        &post(
            &format!("/v1/datasets/{}/export", import.metadata.short_ref),
            br#"{"output_path":"relative/export"}"#,
        ),
        &state,
    )
    .await;
    let relative_body: Value = serde_json::from_slice(response_buffer(&relative)).expect("json");
    assert_eq!(relative.status_code, 400);
    assert_eq!(relative_body["error"], "bad_request");
}

#[tokio::test]
async fn dataset_diff_supports_managed_and_path_right_sides_with_file_cap() {
    let home = unique_home("dataset-diff");
    let left_source = write_dataset_source_with_extra_files(&home, "left", 0);
    let right_source = write_dataset_source_with_extra_files(&home, "right", 2);
    let large_source = write_dataset_source_with_extra_files(&home, "large", 505);
    let manager = DatasetManager::new_with_home(Some(&home)).expect("dataset manager");
    let left = manager.add_path(&left_source).expect("left dataset");
    let right = manager.add_path(&right_source).expect("right dataset");
    let state = state_for(home.clone());

    let managed = route_request(
        &post(
            &format!("/v1/datasets/{}/diff", left.metadata.short_ref),
            format!(r#"{{"right_dataset_ref":"{}"}}"#, right.metadata.short_ref).as_bytes(),
        ),
        &state,
    )
    .await;
    let managed_body: Value = serde_json::from_slice(response_buffer(&managed)).expect("json");
    assert_eq!(managed.status_code, 200);
    assert_eq!(managed_body["left"]["short_ref"], left.metadata.short_ref);
    assert_eq!(managed_body["right"]["short_ref"], right.metadata.short_ref);
    assert_eq!(managed_body["diff"]["summary"]["added"], 2);
    assert_eq!(managed_body["diff"]["truncated"], false);

    let path = route_request(
        &post(
            &format!("/v1/datasets/{}/diff", left.metadata.short_ref),
            format!(r#"{{"right_path":"{}"}}"#, path_string(&large_source)).as_bytes(),
        ),
        &state,
    )
    .await;
    let path_body: Value = serde_json::from_slice(response_buffer(&path)).expect("json");
    assert_eq!(path.status_code, 200);
    assert_eq!(path_body["right"]["path"], path_string(&large_source));
    assert_eq!(path_body["diff"]["file_limit"], 500);
    assert_eq!(path_body["diff"]["truncated"], true);
    assert_eq!(
        path_body["diff"]["files"].as_array().expect("files").len(),
        500
    );

    for payload in [
        r#"{}"#.to_string(),
        format!(
            r#"{{"right_dataset_ref":"{}","right_path":"{}"}}"#,
            right.metadata.short_ref,
            path_string(&right_source)
        ),
        r#"{"right_path":"relative/dataset"}"#.to_string(),
    ] {
        let response = route_request(
            &post(
                &format!("/v1/datasets/{}/diff", left.metadata.short_ref),
                payload.as_bytes(),
            ),
            &state,
        )
        .await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400, "{payload}");
        assert_eq!(body["error"], "bad_request");
    }
}

#[tokio::test]
async fn dataset_tool_routes_are_static_and_protected() {
    let home = unique_home("dataset-tools-static-auth");
    let state = state_for(home.clone());

    for path in [
        "/v1/datasets/validate",
        "/v1/datasets/template",
        "/v1/datasets/synth",
        "/v1/datasets/eval",
        "/v1/datasets/anything/export",
        "/v1/datasets/anything/diff",
    ] {
        let response = route_request(&get(path), &state).await;
        assert_eq!(response.status_code, 405, "{path}");
    }

    let token_state = state_with_token(home, "secret");
    let unauthorized = route_request(
        &post("/v1/datasets/validate", br#"{"path":"/tmp/nope"}"#),
        &token_state,
    )
    .await;
    assert_eq!(unauthorized.status_code, 401);
}

#[tokio::test]
async fn train_plan_preview_create_inspect_list_and_delete_work() {
    let home = unique_home("train-plan-happy");
    let model_ref = "151515151515000000000000";
    let dataset_ref = "252525252525000000000000";
    write_model_fixture(&home, model_ref);
    write_dataset_fixture(&home, dataset_ref);
    let state = state_for(home.clone());

    let preview_payload = json!({
        "model_ref": &model_ref[..12],
        "dataset_ref": &dataset_ref[..12],
        "name": "first plan",
        "backend": "auto",
        "overrides": {
            "rank": 4,
            "learning_rate": 0.0002,
            "batch_size": 1,
            "gradient_accumulation_steps": 2,
            "max_steps": 3
        }
    });
    let preview = route_request(
        &post(
            "/v1/train/lora/plans/preview",
            &serde_json::to_vec(&preview_payload).expect("payload"),
        ),
        &state,
    )
    .await;
    let preview_body: Value = serde_json::from_slice(response_buffer(&preview)).expect("json");
    assert_eq!(preview.status_code, 200);
    assert_eq!(preview_body["preview"]["persisted"], false);
    assert_eq!(preview_body["plan"]["model_ref"], model_ref);
    assert_eq!(preview_body["plan"]["dataset_ref"], dataset_ref);
    assert_eq!(preview_body["plan"]["name"], "first plan");
    assert_eq!(preview_body["plan"]["lora"]["rank"], 4);
    let would_plan_path = preview_body["preview"]["would_plan_path"]
        .as_str()
        .expect("would plan path");
    assert!(!PathBuf::from(would_plan_path).exists());

    let create = route_request(
        &post(
            "/v1/train/lora/plans",
            &serde_json::to_vec(&preview_payload).expect("payload"),
        ),
        &state,
    )
    .await;
    let create_body: Value = serde_json::from_slice(response_buffer(&create)).expect("json");
    assert_eq!(create.status_code, 200);
    assert_eq!(create_body["created"], true);
    assert_eq!(create_body["deduplicated"], false);
    let plan_ref = create_body["plan"]["plan_ref"].as_str().expect("plan ref");
    let short_ref = create_body["plan"]["short_ref"]
        .as_str()
        .expect("short ref");
    assert!(PathBuf::from(create_body["plan_path"].as_str().expect("plan path")).exists());

    let renamed_payload = json!({
        "model_ref": model_ref,
        "dataset_ref": dataset_ref,
        "name": "second name",
        "backend": "auto",
        "overrides": {
            "rank": 4,
            "learning_rate": 0.0002,
            "batch_size": 1,
            "gradient_accumulation_steps": 2,
            "max_steps": 3
        }
    });
    let repeated = route_request(
        &post(
            "/v1/train/lora/plans",
            &serde_json::to_vec(&renamed_payload).expect("payload"),
        ),
        &state,
    )
    .await;
    let repeated_body: Value = serde_json::from_slice(response_buffer(&repeated)).expect("json");
    assert_eq!(repeated.status_code, 200);
    assert_eq!(repeated_body["created"], false);
    assert_eq!(repeated_body["deduplicated"], true);
    assert_eq!(repeated_body["plan"]["plan_ref"], plan_ref);
    assert_eq!(repeated_body["plan"]["name"], "first plan");

    let list = route_request(&get("/v1/train/lora/plans"), &state).await;
    let list_body: Value = serde_json::from_slice(response_buffer(&list)).expect("json");
    assert_eq!(list.status_code, 200);
    assert_eq!(list_body["plans"].as_array().expect("plans").len(), 1);
    assert_eq!(list_body["plans"][0]["plan_ref"], plan_ref);
    assert_eq!(list_body["plans"][0]["run_count"], 0);

    let inspect = route_request(&get(&format!("/v1/train/lora/plans/{short_ref}")), &state).await;
    let inspect_body: Value = serde_json::from_slice(response_buffer(&inspect)).expect("json");
    assert_eq!(inspect.status_code, 200);
    assert_eq!(inspect_body["plan"]["plan_ref"], plan_ref);
    assert_eq!(inspect_body["run_count"], 0);

    let remove = route_request(
        &delete(&format!("/v1/train/lora/plans/{short_ref}"), b""),
        &state,
    )
    .await;
    let remove_body: Value = serde_json::from_slice(response_buffer(&remove)).expect("json");
    assert_eq!(remove.status_code, 200);
    assert_eq!(remove_body["removed"]["kind"], "lora_train_plan");
    assert_eq!(remove_body["removed"]["plan_ref"], plan_ref);
    assert_eq!(remove_body["plan"]["plan_ref"], plan_ref);

    let missing = route_request(&get(&format!("/v1/train/lora/plans/{plan_ref}")), &state).await;
    let missing_body: Value = serde_json::from_slice(response_buffer(&missing)).expect("json");
    assert_eq!(missing.status_code, 404);
    assert_eq!(missing_body["error"], "not_found");
}

#[tokio::test]
async fn train_plan_errors_auth_blocked_and_in_use_are_mapped() {
    let home = unique_home("train-plan-errors");
    let model_ref = "353535353535000000000000";
    let dataset_ref = "454545454545000000000000";
    write_model_fixture(&home, model_ref);
    write_model_fixture(&home, "353535353536000000000000");
    write_dataset_fixture(&home, dataset_ref);
    let state = state_for(home.clone());

    let missing = route_request(
        &post(
            "/v1/train/lora/plans/preview",
            br#"{"model_ref":"missing","dataset_ref":"454545454545"}"#,
        ),
        &state,
    )
    .await;
    let missing_body: Value = serde_json::from_slice(response_buffer(&missing)).expect("json");
    assert_eq!(missing.status_code, 404);
    assert_eq!(missing_body["error"], "not_found");

    let ambiguous = route_request(
        &post(
            "/v1/train/lora/plans/preview",
            br#"{"model_ref":"35353535353","dataset_ref":"454545454545"}"#,
        ),
        &state,
    )
    .await;
    let ambiguous_body: Value = serde_json::from_slice(response_buffer(&ambiguous)).expect("json");
    assert_eq!(ambiguous.status_code, 409);
    assert_eq!(ambiguous_body["error"], "ambiguous_ref");

    for payload in [
        br#"{"model_ref":"353535353535","dataset_ref":"454545454545","backend":"bad"}"#.as_slice(),
        br#"{"model_ref":"353535353535","dataset_ref":"454545454545","extra":true}"#,
        br#"{"model_ref":"353535353535","dataset_ref":"454545454545","overrides":{"rank":0}}"#,
        br#"{"model_ref":"353535353535","dataset_ref":"454545454545","overrides":{"peft_load_in_4bit":true,"peft_load_in_8bit":true}}"#,
        br#"{"model_ref":"../353535353535","dataset_ref":"454545454545"}"#,
    ] {
        let response = route_request(&post("/v1/train/lora/plans/preview", payload), &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400);
        assert_eq!(body["error"], "bad_request");
    }

    let blocked = route_request(
        &post(
            "/v1/train/lora/plans/preview",
            br#"{"model_ref":"353535353535","dataset_ref":"454545454545","backend":"peft"}"#,
        ),
        &state,
    )
    .await;
    let blocked_body: Value = serde_json::from_slice(response_buffer(&blocked)).expect("json");
    assert_eq!(blocked.status_code, 200);
    assert_eq!(blocked_body["plan"]["status"], "blocked");
    assert!(blocked_body["plan"]["blockers"]
        .as_array()
        .expect("blockers")
        .iter()
        .any(|value| value
            .as_str()
            .expect("blocker")
            .contains("requires model primary_format safetensors")));

    let token_state = state_with_token(unique_home("train-plan-token"), "secret");
    let unauthorized = route_request(&get("/v1/train/lora/plans"), &token_state).await;
    assert_eq!(unauthorized.status_code, 401);
    let authorized = route_request(
        &get_with_header("/v1/train/lora/plans", "Authorization", "Bearer secret"),
        &token_state,
    )
    .await;
    assert_eq!(authorized.status_code, 200);

    let static_route = route_request(&get("/v1/train/lora/plans/preview"), &state).await;
    assert_eq!(static_route.status_code, 405);

    let create = route_request(
        &post(
            "/v1/train/lora/plans",
            br#"{"model_ref":"353535353535","dataset_ref":"454545454545"}"#,
        ),
        &state,
    )
    .await;
    let create_body: Value = serde_json::from_slice(response_buffer(&create)).expect("json");
    let plan_ref = create_body["plan"]["plan_ref"].as_str().expect("plan ref");
    write_train_run_fixture(&home, plan_ref, "run-1");

    let delete_body = route_request(
        &delete(&format!("/v1/train/lora/plans/{plan_ref}"), b"{}"),
        &state,
    )
    .await;
    let delete_body_json: Value =
        serde_json::from_slice(response_buffer(&delete_body)).expect("json");
    assert_eq!(delete_body.status_code, 400);
    assert_eq!(delete_body_json["error"], "bad_request");

    let in_use = route_request(
        &delete(&format!("/v1/train/lora/plans/{plan_ref}"), b""),
        &state,
    )
    .await;
    let in_use_body: Value = serde_json::from_slice(response_buffer(&in_use)).expect("json");
    assert_eq!(in_use.status_code, 409);
    assert_eq!(in_use_body["error"], "in_use");
}

#[tokio::test]
async fn train_run_list_inspect_metrics_and_logs_work() {
    let home = unique_home("train-run-read");
    let model_ref = "555555555555000000000000";
    let dataset_ref = "656565656565000000000000";
    write_model_fixture(&home, model_ref);
    write_dataset_fixture(&home, dataset_ref);
    let state = state_for(home.clone());

    let create = route_request(
        &post(
            "/v1/train/lora/plans",
            br#"{"model_ref":"555555555555","dataset_ref":"656565656565","backend":"auto"}"#,
        ),
        &state,
    )
    .await;
    let create_body: Value = serde_json::from_slice(response_buffer(&create)).expect("json");
    assert_eq!(create.status_code, 200);
    let plan_ref = create_body["plan"]["plan_ref"].as_str().expect("plan ref");
    let run_ref = "777777777777000000000000";
    write_train_run_fixture_full(&home, plan_ref, run_ref, model_ref, dataset_ref);

    let global = route_request(&get("/v1/train/lora/runs"), &state).await;
    let global_body: Value = serde_json::from_slice(response_buffer(&global)).expect("json");
    assert_eq!(global.status_code, 200);
    assert_eq!(global_body["runs"].as_array().expect("runs").len(), 1);
    assert_eq!(global_body["runs"][0]["run_ref"], run_ref);
    assert_eq!(global_body["runs"][0]["status"], "succeeded");

    let plan_runs = route_request(
        &get(&format!("/v1/train/lora/plans/{}/runs", &plan_ref[..12])),
        &state,
    )
    .await;
    let plan_runs_body: Value = serde_json::from_slice(response_buffer(&plan_runs)).expect("json");
    assert_eq!(plan_runs.status_code, 200);
    assert_eq!(plan_runs_body["runs"][0]["plan_ref"], plan_ref);

    let inspect = route_request(&get("/v1/train/lora/runs/777777777777"), &state).await;
    let inspect_body: Value = serde_json::from_slice(response_buffer(&inspect)).expect("json");
    assert_eq!(inspect.status_code, 200);
    assert_eq!(inspect_body["run"]["run_ref"], run_ref);
    assert_eq!(inspect_body["run"]["process_running"], false);
    assert_eq!(inspect_body["run"]["stale"], false);

    let metrics = route_request(
        &get("/v1/train/lora/runs/777777777777/metrics?tail=1"),
        &state,
    )
    .await;
    let metrics_body: Value = serde_json::from_slice(response_buffer(&metrics)).expect("json");
    assert_eq!(metrics.status_code, 200);
    assert_eq!(metrics_body["total_events"], 2);
    assert_eq!(metrics_body["truncated"], true);
    assert_eq!(metrics_body["events"].as_array().expect("events").len(), 1);
    assert_eq!(metrics_body["events"][0]["index"], 1);
    assert_eq!(metrics_body["warnings"][0]["code"], "malformed_metric");
    assert!(!metrics_body["warnings"][0]["message"]
        .as_str()
        .expect("warning")
        .contains("not-json"));

    let invalid_tail = route_request(
        &get("/v1/train/lora/runs/777777777777/metrics?tail=0"),
        &state,
    )
    .await;
    assert_eq!(invalid_tail.status_code, 400);

    let logs = route_request(&get("/v1/train/lora/runs/777777777777/logs"), &state).await;
    let logs_body: Value = serde_json::from_slice(response_buffer(&logs)).expect("json");
    assert_eq!(logs.status_code, 200);
    assert_eq!(logs_body["logs"]["raw"]["exists"], true);

    let raw = route_request(
        &get("/v1/train/lora/runs/777777777777/logs/raw?tail_bytes=4"),
        &state,
    )
    .await;
    let raw_body: Value = serde_json::from_slice(response_buffer(&raw)).expect("json");
    assert_eq!(raw.status_code, 200);
    assert_eq!(raw_body["log"]["kind"], "raw");
    assert_eq!(raw_body["log"]["content"], "ne2\n");

    let bad_start_body = route_request(
        &post(
            &format!("/v1/train/lora/plans/{}/runs", &plan_ref[..12]),
            br#"{"force":true}"#,
        ),
        &state,
    )
    .await;
    assert_eq!(bad_start_body.status_code, 400);
}

#[tokio::test]
async fn dataset_synth_route_validates_modes_before_provider_calls() {
    let home = unique_home("dataset-synth-route-validation");
    fs::create_dir_all(&home).expect("home");
    let state = state_for(home.clone());
    let spec = home.join("spec.md");
    fs::write(&spec, "Generate records.").expect("spec");
    let output = home.join("out");

    for payload in [
        r#"{}"#.to_string(),
        r#"{"print_prompt":true,"brief":"x","provider":"openai","split":"train","count":1}"#
            .to_string(),
        r#"{"brief":"x","spec_content":"y","split":"train","count":1}"#.to_string(),
        r#"{"provider":"openai","model":"m","output_path":"/tmp/out","brief":"x","split":"train"}"#
            .to_string(),
        r#"{"provider":"openai","model":"m","output_path":"/tmp/out","brief":"x","train_count":0,"valid_count":0}"#
            .to_string(),
        r#"{"provider":"openai","model":"m","output_path":"/tmp/out","spec_path":"relative.md","split":"train","count":1}"#
            .to_string(),
    ] {
        let response = route_request(&post("/v1/datasets/synth", payload.as_bytes()), &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400, "{payload}");
        assert_eq!(body["error"], "bad_request");
    }

    let unsupported_provider = route_request(
        &post(
            "/v1/datasets/synth",
            format!(
                r#"{{"provider":"bogus","model":"m","output_path":"{}","spec_path":"{}","split":"train","count":1}}"#,
                path_string(&output),
                path_string(&spec)
            )
            .as_bytes(),
        ),
        &state,
    )
    .await;
    let unsupported_body: Value =
        serde_json::from_slice(response_buffer(&unsupported_provider)).expect("json");
    assert_eq!(unsupported_provider.status_code, 400);
    assert_eq!(unsupported_body["error"], "bad_request");
}

#[tokio::test]
async fn dataset_eval_route_validates_sources_content_and_refs() {
    let home = unique_home("dataset-eval-route-validation");
    let state = state_for(home.clone());
    let output = home.join("eval-report");

    for payload in [
        format!(
            r#"{{"provider":"openai","model":"m","output_path":"{}"}}"#,
            path_string(&output)
        ),
        format!(
            r#"{{"provider":"openai","model":"m","output_path":"{}","dataset_ref":"abc","input_content":"x"}}"#,
            path_string(&output)
        ),
        format!(
            r#"{{"provider":"openai","model":"m","output_path":"{}","input_path":"relative.jsonl"}}"#,
            path_string(&output)
        ),
        format!(
            r#"{{"provider":"openai","model":"m","output_path":"{}","input_content":"x","input_format":"csv"}}"#,
            path_string(&output)
        ),
        format!(
            r#"{{"provider":"openai","model":"m","output_path":"{}","input_content":"x","max_records":0}}"#,
            path_string(&output)
        ),
    ] {
        let response = route_request(&post("/v1/datasets/eval", payload.as_bytes()), &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400, "{payload}");
        assert_eq!(body["error"], "bad_request");
    }

    let missing_ref = route_request(
        &post(
            "/v1/datasets/eval",
            format!(
                r#"{{"provider":"openai","model":"m","output_path":"{}","dataset_ref":"missing"}}"#,
                path_string(&output)
            )
            .as_bytes(),
        ),
        &state,
    )
    .await;
    let missing_ref_body: Value =
        serde_json::from_slice(response_buffer(&missing_ref)).expect("json");
    assert_eq!(missing_ref.status_code, 404);
    assert_eq!(missing_ref_body["error"], "not_found");

    let unsupported_provider = route_request(
        &post(
            "/v1/datasets/eval",
            format!(
                r#"{{"provider":"bogus","model":"m","output_path":"{}","input_content":"{{}}\n"}}"#,
                path_string(&output)
            )
            .as_bytes(),
        ),
        &state,
    )
    .await;
    let unsupported_body: Value =
        serde_json::from_slice(response_buffer(&unsupported_provider)).expect("json");
    assert_eq!(unsupported_provider.status_code, 400);
    assert_eq!(unsupported_body["error"], "bad_request");
}

#[tokio::test]
async fn store_mutation_routes_are_static_and_protected() {
    let home = unique_home("store-mutation-static-auth");
    let state = state_for(home.clone());

    let get_import = route_request(&get("/v1/models/import"), &state).await;
    assert_eq!(get_import.status_code, 405);

    let delete_pull = route_request(&delete("/v1/models/pull", b""), &state).await;
    assert_eq!(delete_pull.status_code, 405);

    let token_state = state_with_token(home, "secret");
    let unauthorized = route_request(
        &post("/v1/datasets/import", br#"{"path":"/tmp/nope"}"#),
        &token_state,
    )
    .await;
    assert_eq!(unauthorized.status_code, 401);
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

    let method = route_request(&post("/v1/sessions/aaaaaaaaaaaa", b"{}"), &state).await;
    assert_eq!(method.status_code, 405);

    let token_state = state_with_token(unique_home("sessions-token"), "secret");
    let unauthorized = route_request(&get("/v1/sessions"), &token_state).await;
    assert_eq!(unauthorized.status_code, 401);
}

#[tokio::test]
async fn session_mutation_routes_create_update_append_and_delete() {
    let home = unique_home("sessions-mutate-http");
    let state = state_for(home.clone());

    let create = route_request(
        &post(
            "/v1/sessions",
            br#"{"title":" Plan ","tags":[" alpha ","Beta"],"messages":[{"role":"system","content":"Be useful."}]}"#,
        ),
        &state,
    )
    .await;
    let create_body: Value = serde_json::from_slice(response_buffer(&create)).expect("json");
    assert_eq!(create.status_code, 201);
    assert_eq!(create_body["created"], true);
    assert_eq!(create_body["session"]["title"], "Plan");
    assert_eq!(create_body["session"]["message_count"], 1);
    assert_eq!(create_body["session"]["tags"], json!(["alpha", "Beta"]));
    let short_ref = create_body["session"]["short_ref"]
        .as_str()
        .expect("short_ref")
        .to_string();

    let patch_response = route_request(
        &patch(
            &format!("/v1/sessions/{short_ref}"),
            br#"{"title":"Updated","tags":[]}"#,
        ),
        &state,
    )
    .await;
    let patch_body: Value = serde_json::from_slice(response_buffer(&patch_response)).expect("json");
    assert_eq!(patch_response.status_code, 200);
    assert_eq!(patch_body["session"]["title"], "Updated");
    assert_eq!(patch_body["session"]["tags"], json!([]));

    let append = route_request(
        &post(
            &format!("/v1/sessions/{short_ref}/messages"),
            br#"{"messages":[{"role":"user","content":"Hello","metadata":{"via":"http"}},{"role":"assistant","content":"Hi"}]}"#,
        ),
        &state,
    )
    .await;
    let append_body: Value = serde_json::from_slice(response_buffer(&append)).expect("json");
    assert_eq!(append.status_code, 200);
    assert_eq!(append_body["session"]["message_count"], 3);
    assert_eq!(append_body["appended"][0]["index"], 1);
    assert_eq!(append_body["appended"][1]["index"], 2);

    let messages = route_request(
        &get(&format!("/v1/sessions/{short_ref}/messages?tail=10")),
        &state,
    )
    .await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 3);
    assert_eq!(
        messages_body["messages"][1]["metadata"],
        json!({"via":"http"})
    );
    assert_eq!(messages_body["messages"][2]["metadata"], json!({}));

    let remove = route_request(&delete(&format!("/v1/sessions/{short_ref}"), b""), &state).await;
    let remove_body: Value = serde_json::from_slice(response_buffer(&remove)).expect("json");
    assert_eq!(remove.status_code, 200);
    assert_eq!(remove_body["removed"]["kind"], "session");
    assert_eq!(remove_body["session"]["short_ref"], short_ref);

    let missing = route_request(&get(&format!("/v1/sessions/{short_ref}")), &state).await;
    assert_eq!(missing.status_code, 404);
    assert!(!home
        .join("sessions")
        .join(remove_body["removed"]["session_ref"].as_str().unwrap())
        .exists());
}

#[tokio::test]
async fn session_compact_route_rewrites_to_summary_plus_recent() {
    let (port, received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"text":"summary of older messages","stream":false}"#,
        true,
    )
    .await;
    let home = unique_home("session-compact");
    let server_ref = create_running_cloud_server(&home, port);
    write_session_fixture_with_refs(
        &home,
        "515151515151000000000000",
        "Compact",
        Some(&server_ref),
        None,
        60,
        Some(&session_messages_n(60)),
    );
    let state = state_for(home.clone());
    let response =
        route_request(&post("/v1/sessions/515151515151/compact", br#"{}"#), &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let proxied_body: Value =
        serde_json::from_slice(&received_body.await.expect("mock body")).expect("proxied json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["session"]["message_count"], 50);
    assert_eq!(body["compaction"]["compacted"], true);
    assert_eq!(body["compaction"]["replaced_message_count"], 11);
    assert!(proxied_body["messages"][0]["content"]
        .as_str()
        .expect("prompt")
        .contains("Summarize the session transcript"));

    let messages = route_request(&get("/v1/sessions/515151515151/messages?tail=60"), &state).await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 50);
    assert_eq!(messages_body["messages"][0]["role"], "system");
    assert_eq!(
        messages_body["messages"][0]["metadata"]["kind"],
        "session_summary"
    );
    assert_eq!(messages_body["messages"][1]["content"], "message 11");
}

#[tokio::test]
async fn session_append_over_cap_requires_compaction_server() {
    let home = unique_home("session-append-over-cap");
    write_session_fixture(
        &home,
        "525252525252000000000000",
        "Append",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        50,
        Some(&session_messages_n(50)),
    );
    let state = state_for(home);
    let response = route_request(
        &post(
            "/v1/sessions/525252525252/messages",
            br#"{"messages":[{"role":"user","content":"new"}]}"#,
        ),
        &state,
    )
    .await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");

    assert_eq!(response.status_code, 409);
    assert_eq!(body["error"], "session_compaction_required");
}

#[tokio::test]
async fn session_mutation_validation_auth_and_methods_map_errors() {
    let home = unique_home("sessions-mutate-errors");
    let state = state_for(home.clone());
    let create = route_request(&post("/v1/sessions", br#"{"title":"Base"}"#), &state).await;
    let create_body: Value = serde_json::from_slice(response_buffer(&create)).expect("json");
    let short_ref = create_body["session"]["short_ref"]
        .as_str()
        .expect("short_ref")
        .to_string();

    for request in [
        patch(&format!("/v1/sessions/{short_ref}"), br#"{}"#),
        patch(&format!("/v1/sessions/{short_ref}"), br#"{"title":"   "}"#),
        patch(
            &format!("/v1/sessions/{short_ref}"),
            br#"{"tags":["x"," x "]}"#,
        ),
        post(
            &format!("/v1/sessions/{short_ref}/messages"),
            br#"{"messages":[{"role":"alien","content":"Hello"}]}"#,
        ),
        post(
            &format!("/v1/sessions/{short_ref}/messages"),
            br#"{"messages":[{"role":"user","content":"Hello","metadata":null}]}"#,
        ),
        delete(&format!("/v1/sessions/{short_ref}"), br#"{}"#),
    ] {
        let response = route_request(&request, &state).await;
        let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
        assert_eq!(response.status_code, 400);
        assert_eq!(body["error"], "bad_request");
    }

    let path_like = route_request(&get("/v1/sessions/../bad"), &state).await;
    assert_eq!(path_like.status_code, 404);

    let token_state = state_with_token(unique_home("sessions-mutate-token"), "secret");
    let unauthorized =
        route_request(&post("/v1/sessions", br#"{"title":"Nope"}"#), &token_state).await;
    assert_eq!(unauthorized.status_code, 401);

    let authorized = route_request(
        &post_with_header(
            "/v1/sessions",
            br#"{"title":"Ok"}"#,
            "Authorization",
            "Bearer secret",
        ),
        &token_state,
    )
    .await;
    assert_eq!(authorized.status_code, 201);
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
async fn server_delete_removes_stopped_specs_and_rejects_running_or_body() {
    let home = unique_home("server-delete");
    let manager = ServerManager::new(Some(&home)).expect("server manager");
    let stopped = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: "openai:gpt-4.1-mini".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(8796),
            lazy_load: false,
            idle_seconds: None,
        })
        .expect("stopped server");
    let running_ref = create_running_cloud_server(&home, 8797);
    let state = state_for(home.clone());

    let body = route_request(
        &delete(&format!("/v1/servers/{}", stopped.spec.short_ref), b"{}"),
        &state,
    )
    .await;
    let body_json: Value = serde_json::from_slice(response_buffer(&body)).expect("json");
    assert_eq!(body.status_code, 400);
    assert_eq!(body_json["error"], "bad_request");

    let running = route_request(&delete(&format!("/v1/servers/{running_ref}"), b""), &state).await;
    let running_body: Value = serde_json::from_slice(response_buffer(&running)).expect("json");
    assert_eq!(running.status_code, 409);
    assert_eq!(running_body["error"], "already_running");

    let remove = route_request(
        &delete(&format!("/v1/servers/{}", stopped.spec.short_ref), b""),
        &state,
    )
    .await;
    let remove_body: Value = serde_json::from_slice(response_buffer(&remove)).expect("json");
    assert_eq!(remove.status_code, 200);
    assert_eq!(remove_body["removed"]["kind"], "server");
    assert_eq!(
        remove_body["removed"]["server_ref"],
        stopped.spec.server_ref
    );
    assert_eq!(remove_body["server"]["server_ref"], stopped.spec.server_ref);

    let missing = route_request(
        &get(&format!("/v1/servers/{}", stopped.spec.short_ref)),
        &state,
    )
    .await;
    let missing_body: Value = serde_json::from_slice(response_buffer(&missing)).expect("json");
    assert_eq!(missing.status_code, 404);
    assert_eq!(missing_body["error"], "not_found");
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
async fn native_chat_session_uses_context_default_server_and_appends() {
    let (port, received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"text":"session hello","stream":false}"#,
        true,
    )
    .await;
    let home = unique_home("native-chat-session");
    let server_ref = create_running_cloud_server(&home, port);
    write_session_fixture_with_refs(
        &home,
        "919191919191000000000000",
        "Chat",
        Some(&server_ref),
        None,
        1,
        Some(&[session_message("user", "historical")]),
    );
    let request = post(
        "/v1/chat",
        br#"{"session_ref":"919191919191","messages":[{"role":"user","content":"new"}],"stream":false}"#,
    );
    let state = state_for(home.clone());
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let proxied_body: Value =
        serde_json::from_slice(&received_body.await.expect("mock body")).expect("proxied json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["text"], "session hello");
    assert_eq!(
        proxied_body["messages"].as_array().expect("messages").len(),
        2
    );
    assert_eq!(proxied_body["messages"][0]["content"], "historical");
    assert_eq!(proxied_body["messages"][1]["content"], "new");
    assert!(proxied_body.get("session_ref").is_none());
    assert!(proxied_body.get("server_ref").is_none());

    let messages = route_request(&get("/v1/sessions/919191919191/messages?tail=10"), &state).await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 3);
    assert_eq!(messages_body["messages"][2]["role"], "assistant");
    assert_eq!(messages_body["messages"][2]["content"], "session hello");
    assert_eq!(messages_body["messages"][2]["metadata"]["route"], "native");
    assert_eq!(
        messages_body["messages"][2]["metadata"]["server_ref"],
        expanded_server_ref(&home, &server_ref)
    );
}

#[tokio::test]
async fn native_chat_session_uses_request_scoped_summary_context() {
    let (port, received_bodies) = spawn_mock_chat_server_sequence(vec![
        MockChatResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            response_body: br#"{"text":"native request summary"}"#,
            include_content_length: true,
        },
        MockChatResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            response_body: br#"{"text":"session hello","stream":false}"#,
            include_content_length: true,
        },
    ])
    .await;
    let home = unique_home("native-chat-session-request-summary");
    let server_ref = create_running_cloud_server(&home, port);
    write_session_fixture_with_refs(
        &home,
        "949494949494000000000000",
        "Chat",
        Some(&server_ref),
        None,
        4,
        Some(&[
            session_message("user", "old fact"),
            session_message("assistant", "old answer"),
            session_message("user", "recent question"),
            session_message("assistant", "recent answer"),
        ]),
    );
    let request = post(
        "/v1/chat",
        br#"{"session_ref":"949494949494","max_session_messages":2,"messages":[{"role":"user","content":"new"}],"stream":false}"#,
    );
    let state = state_for(home.clone());
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let received_bodies = received_bodies.await.expect("mock bodies");
    let summary_body: Value = serde_json::from_slice(&received_bodies[0]).expect("summary json");
    let proxied_body: Value = serde_json::from_slice(&received_bodies[1]).expect("proxied json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["text"], "session hello");
    assert!(summary_body["messages"][0]["content"]
        .as_str()
        .expect("summary system")
        .contains("request-scoped"));
    assert!(summary_body["messages"][1]["content"]
        .as_str()
        .expect("summary user")
        .contains("[current 0] user: new"));
    assert_eq!(
        proxied_body["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|message| {
                (
                    message["role"].as_str().expect("role"),
                    message["content"].as_str().expect("content"),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("system", "native request summary"),
            ("assistant", "recent answer"),
            ("user", "new"),
        ]
    );
    assert!(proxied_body.get("session_ref").is_none());
    assert!(proxied_body.get("max_session_messages").is_none());
    assert!(proxied_body.get("server_ref").is_none());

    let messages = route_request(&get("/v1/sessions/949494949494/messages?tail=10"), &state).await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 6);
    assert!(!messages_body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .any(|message| message["content"] == "native request summary"));
}

#[tokio::test]
async fn native_chat_session_stream_appends_before_done() {
    let sse = b"event: delta\ndata: {\"delta\":\"hi\"}\n\nevent: done\ndata: {\"finish_reason\":\"stop\"}\n\n";
    let (port, _received_body) =
        spawn_mock_chat_server(200, "text/event-stream; charset=utf-8", sse, false).await;
    let home = unique_home("native-chat-session-stream");
    let server_ref = create_running_cloud_server(&home, port);
    write_session_fixture_with_refs(
        &home,
        "929292929292000000000000",
        "Chat",
        Some(&server_ref),
        None,
        0,
        None,
    );
    let request = post(
        "/v1/chat",
        br#"{"session_ref":"929292929292","messages":[{"role":"user","content":"new"}],"stream":true}"#,
    );
    let state = state_for(home);
    let response = route_request(&request, &state).await;
    let body = String::from_utf8(collect_response_body(response).await).expect("utf8");

    assert!(body.contains(r#"event: delta"#));
    assert!(body.contains(r#"event: done"#));
    let messages = route_request(&get("/v1/sessions/929292929292/messages"), &state).await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 2);
    assert_eq!(messages_body["messages"][1]["content"], "hi");
}

#[tokio::test]
async fn native_chat_session_stream_uses_request_summary_without_persisting_it() {
    let sse = b"event: delta\ndata: {\"delta\":\"hi\"}\n\nevent: done\ndata: {\"finish_reason\":\"stop\"}\n\n";
    let (port, received_bodies) = spawn_mock_chat_server_sequence(vec![
        MockChatResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            response_body: br#"{"text":"stream request summary"}"#,
            include_content_length: true,
        },
        MockChatResponse {
            status: 200,
            content_type: "text/event-stream; charset=utf-8",
            response_body: sse,
            include_content_length: false,
        },
    ])
    .await;
    let home = unique_home("native-chat-session-stream-summary");
    let server_ref = create_running_cloud_server(&home, port);
    write_session_fixture_with_refs(
        &home,
        "959595959595000000000000",
        "Chat",
        Some(&server_ref),
        None,
        2,
        Some(&[
            session_message("user", "old fact"),
            session_message("assistant", "old answer"),
        ]),
    );
    let request = post(
        "/v1/chat",
        br#"{"session_ref":"959595959595","max_session_messages":1,"messages":[{"role":"user","content":"new"}],"stream":true}"#,
    );
    let state = state_for(home.clone());
    let response = route_request(&request, &state).await;
    let body = String::from_utf8(collect_response_body(response).await).expect("utf8");
    let received_bodies = received_bodies.await.expect("mock bodies");
    let proxied_body: Value = serde_json::from_slice(&received_bodies[1]).expect("proxied json");

    assert!(body.contains(r#"event: delta"#));
    assert!(body.contains(r#"event: done"#));
    assert_eq!(
        proxied_body["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|message| message["content"].as_str().expect("content"))
            .collect::<Vec<_>>(),
        vec!["stream request summary", "new"]
    );
    assert!(proxied_body.get("session_ref").is_none());
    assert!(proxied_body.get("max_session_messages").is_none());

    let messages = route_request(&get("/v1/sessions/959595959595/messages"), &state).await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 4);
    assert_eq!(messages_body["messages"][3]["content"], "hi");
    assert!(!messages_body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .any(|message| message["content"] == "stream request summary"));
}

#[tokio::test]
async fn session_chat_rejects_invalid_context_options_and_tool_messages() {
    let state = state_for(unique_home("session-chat-invalid"));
    let bad_max = route_request(
        &post(
            "/v1/chat",
            br#"{"messages":[{"role":"user","content":"Hello"}],"max_session_messages":1}"#,
        ),
        &state,
    )
    .await;
    let bad_max_body: Value = serde_json::from_slice(response_buffer(&bad_max)).expect("json");
    assert_eq!(bad_max.status_code, 400);
    assert_eq!(bad_max_body["error"], "bad_request");

    let tool_request = route_request(
        &post(
            "/v1/chat",
            br#"{"session_ref":"abc","messages":[{"role":"tool","content":"nope"}]}"#,
        ),
        &state,
    )
    .await;
    assert_eq!(tool_request.status_code, 400);
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
async fn openai_chat_completions_session_appends_and_keeps_model_required() {
    let (port, received_body) = spawn_mock_chat_server(
        200,
        "application/json; charset=utf-8",
        br#"{"text":"openai session hello","stream":false}"#,
        true,
    )
    .await;
    let home = unique_home("openai-chat-session");
    let server_ref = create_running_cloud_server(&home, port);
    write_session_fixture(
        &home,
        "939393939393000000000000",
        "Chat",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        1,
        Some(&[session_message("user", "historical")]),
    );
    let state = state_for(home.clone());

    let missing_model = route_request(
        &post(
            "/v1/chat/completions",
            br#"{"session_ref":"939393939393","messages":[{"role":"user","content":"new"}]}"#,
        ),
        &state,
    )
    .await;
    assert_eq!(missing_model.status_code, 400);

    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","session_ref":"939393939393","messages":[{{"role":"user","content":"new"}}],"stream":false}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let proxied_body: Value =
        serde_json::from_slice(&received_body.await.expect("mock body")).expect("proxied json");

    assert_eq!(response.status_code, 200);
    assert_eq!(body["model"], expanded_server_ref(&home, &server_ref));
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "openai session hello"
    );
    assert_eq!(
        proxied_body["messages"].as_array().expect("messages").len(),
        2
    );
    assert_eq!(proxied_body["messages"][0]["content"], "historical");
    assert!(proxied_body.get("session_ref").is_none());

    let messages = route_request(&get("/v1/sessions/939393939393/messages"), &state).await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 3);
    assert_eq!(
        messages_body["messages"][2]["metadata"]["route"],
        "openai_compat"
    );
}

#[tokio::test]
async fn openai_chat_completions_session_uses_request_scoped_summary_context() {
    let (port, received_bodies) = spawn_mock_chat_server_sequence(vec![
        MockChatResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            response_body: br#"{"text":"openai request summary"}"#,
            include_content_length: true,
        },
        MockChatResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            response_body: br#"{"text":"openai session hello","stream":false}"#,
            include_content_length: true,
        },
    ])
    .await;
    let home = unique_home("openai-chat-session-request-summary");
    let server_ref = create_running_cloud_server(&home, port);
    write_session_fixture(
        &home,
        "969696969696000000000000",
        "Chat",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        4,
        Some(&[
            session_message("user", "old fact"),
            session_message("assistant", "old answer"),
            session_message("user", "recent question"),
            session_message("assistant", "recent answer"),
        ]),
    );
    let state = state_for(home.clone());
    let request = post(
        "/v1/chat/completions",
        format!(
            r#"{{"model":"{}","session_ref":"969696969696","max_session_messages":2,"messages":[{{"role":"user","content":"new"}}],"stream":false,"max_tokens":16}}"#,
            server_ref
        )
        .as_bytes(),
    );
    let response = route_request(&request, &state).await;
    let body: Value = serde_json::from_slice(response_buffer(&response)).expect("json");
    let received_bodies = received_bodies.await.expect("mock bodies");
    let summary_body: Value = serde_json::from_slice(&received_bodies[0]).expect("summary json");
    let proxied_body: Value = serde_json::from_slice(&received_bodies[1]).expect("proxied json");

    assert_eq!(response.status_code, 200);
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "openai session hello"
    );
    assert!(summary_body["messages"][1]["content"]
        .as_str()
        .expect("summary user")
        .contains("[current 0] user: new"));
    assert_eq!(
        proxied_body["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|message| {
                (
                    message["role"].as_str().expect("role"),
                    message["content"].as_str().expect("content"),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("system", "openai request summary"),
            ("assistant", "recent answer"),
            ("user", "new"),
        ]
    );
    assert_eq!(proxied_body["max_tokens"], 16);
    assert!(proxied_body.get("model").is_none());
    assert!(proxied_body.get("session_ref").is_none());
    assert!(proxied_body.get("max_session_messages").is_none());

    let messages = route_request(&get("/v1/sessions/969696969696/messages"), &state).await;
    let messages_body: Value = serde_json::from_slice(response_buffer(&messages)).expect("json");
    assert_eq!(messages_body["total_messages"], 6);
    assert!(!messages_body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .any(|message| message["content"] == "openai request summary"));
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

fn post_with_header(path: &str, body: &[u8], name: &str, value: &str) -> HttpRequest {
    let mut request = post(path, body);
    request.headers.push((name.to_string(), value.to_string()));
    request
}

fn patch(path: &str, body: &[u8]) -> HttpRequest {
    let (path, query_params) = test_split_target(path);
    HttpRequest {
        method: "PATCH".to_string(),
        path,
        query_params,
        version: "HTTP/1.1".to_string(),
        headers: Vec::new(),
        body: body.to_vec(),
        parse_error: None,
    }
}

fn restore_env(name: &str, value: Option<String>) {
    if let Some(value) = value {
        std::env::set_var(name, value);
    } else {
        std::env::remove_var(name);
    }
}

fn delete(path: &str, body: &[u8]) -> HttpRequest {
    let (path, query_params) = test_split_target(path);
    HttpRequest {
        method: "DELETE".to_string(),
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

fn write_local_server_spec_for_model(home: &Path, server_ref: &str, model_ref: &str) {
    let server_dir = home.join("servers").join(server_ref);
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(
        server_dir.join("server.toml"),
        format!(
            r#"server_ref = "{server_ref}"
short_ref = "{server_ref}"
runtime_kind = "local"
model_ref = "{model_ref}"
host = "127.0.0.1"
port = 8000
lazy_load = false
created_at = "2026-05-01T00:00:00Z"
"#
        ),
    )
    .expect("server spec");
}

fn write_cloud_server_spec_with_adapter(home: &Path, server_ref: &str, adapter_ref: &str) {
    let server_dir = home.join("servers").join(server_ref);
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(
        server_dir.join("server.toml"),
        format!(
            r#"server_ref = "{server_ref}"
short_ref = "{server_ref}"
runtime_kind = "cloud"
provider = "openai"
provider_model = "gpt-4.1-mini"
adapter_ref = "{adapter_ref}"
host = "127.0.0.1"
port = 8000
lazy_load = false
created_at = "2026-05-01T00:00:00Z"
"#
        ),
    )
    .expect("server spec");
}

fn write_model_fixture(home: &Path, model_ref: &str) {
    let store_dir = home.join("models/store").join(model_ref);
    fs::create_dir_all(store_dir.join("variants/mlx/source")).expect("model source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    fs::write(
        store_dir.join("model.toml"),
        format!(
            r#"model_ref = "{model_ref}"
short_ref = "{}"
source_kind = "local"
source_path = "{}"
primary_format = "mlx"
detected_formats = ["mlx"]
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
"#,
            &model_ref[..12],
            path_string(&home.join("fixtures/model"))
        ),
    )
    .expect("model metadata");
}

fn write_model_import_source(home: &Path) -> PathBuf {
    let source = home.join("fixtures/import-model");
    fs::create_dir_all(&source).expect("model import source dir");
    fs::write(source.join("model.safetensors"), b"model").expect("model import file");
    source.canonicalize().expect("canonical model source")
}

fn write_adapter_fixture(home: &Path, adapter_ref: &str) {
    let store_dir = home.join("adapters/store").join(adapter_ref);
    fs::create_dir_all(store_dir.join("source")).expect("adapter source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    fs::write(
        store_dir.join("adapter.toml"),
        format!(
            r#"adapter_ref = "{adapter_ref}"
short_ref = "{}"
adapter_format = "mlx"
adapter_type = "lora"
backend_support = []
source_kind = "local"
source_path = "{}"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
"#,
            &adapter_ref[..12],
            path_string(&home.join("fixtures/adapter"))
        ),
    )
    .expect("adapter metadata");
}

fn write_adapter_import_source(home: &Path) -> PathBuf {
    let source = home.join("fixtures/import-adapter");
    fs::create_dir_all(&source).expect("adapter import source dir");
    fs::write(source.join("adapter_model.safetensors"), b"adapter").expect("adapter import file");
    source.canonicalize().expect("canonical adapter source")
}

fn write_dataset_fixture(home: &Path, dataset_ref: &str) {
    let store_dir = home.join("datasets/store").join(dataset_ref);
    fs::create_dir_all(store_dir.join("source")).expect("dataset source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    fs::write(
        store_dir.join("dataset.toml"),
        format!(
            r#"dataset_ref = "{dataset_ref}"
short_ref = "{}"
source_kind = "local"
source_path = "{}"
dataset_format = "directory"
file_count = 2
total_bytes = 20
imported_at = "2026-05-01T00:00:00Z"

[package]
tuning_ready = true
warnings = []

[package.splits]
train = "train.jsonl"
"#,
            &dataset_ref[..12],
            path_string(&home.join("fixtures/dataset"))
        ),
    )
    .expect("dataset metadata");
}

fn write_dataset_import_source(home: &Path) -> PathBuf {
    let source = home.join("fixtures/import-dataset");
    fs::create_dir_all(&source).expect("dataset import source dir");
    fs::write(
        source.join("train.jsonl"),
        r#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]}"#,
    )
    .expect("dataset import file");
    source.canonicalize().expect("canonical dataset source")
}

fn write_invalid_dataset_source(home: &Path) -> PathBuf {
    let source = home.join("fixtures/invalid-dataset");
    fs::create_dir_all(&source).expect("invalid dataset dir");
    fs::write(source.join("train.jsonl"), r#"{"not_messages":true}"#)
        .expect("invalid dataset file");
    source.canonicalize().expect("canonical invalid dataset")
}

fn write_dataset_source_with_extra_files(home: &Path, label: &str, extra_files: usize) -> PathBuf {
    let source = home.join(format!("fixtures/diff-{label}"));
    fs::create_dir_all(&source).expect("diff dataset dir");
    fs::write(
        source.join("train.jsonl"),
        r#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]}"#,
    )
    .expect("diff train file");
    for index in 0..extra_files {
        fs::write(
            source.join(format!("extra-{index:03}.txt")),
            format!("extra {index}"),
        )
        .expect("extra file");
    }
    source.canonicalize().expect("canonical diff dataset")
}

fn write_train_run_fixture(home: &Path, plan_ref: &str, run_ref: &str) {
    let run_dir = home
        .join("train/lora/plans")
        .join(plan_ref)
        .join("runs")
        .join(run_ref);
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::write(run_dir.join("run.toml"), "status = \"running\"\n").expect("run metadata");
}

fn write_train_run_fixture_full(
    home: &Path,
    plan_ref: &str,
    run_ref: &str,
    model_ref: &str,
    dataset_ref: &str,
) {
    let run_dir = home
        .join("train/lora/plans")
        .join(plan_ref)
        .join("runs")
        .join(run_ref);
    fs::create_dir_all(&run_dir).expect("run dir");
    let run_path = run_dir.join("run.toml");
    let metrics_path = run_dir.join("metrics.jsonl");
    let raw_log_path = run_dir.join("raw.log");
    fs::write(
        &run_path,
        format!(
            r#"schema_version = 1
run_ref = "{run_ref}"
short_ref = "{}"
status = "succeeded"
phase = "done"
created_at = "2026-05-01T00:00:00Z"
started_at = "2026-05-01T00:00:00Z"
ended_at = "2026-05-01T00:00:01Z"
plan_ref = "{plan_ref}"
plan_short_ref = "{}"
model_ref = "{model_ref}"
dataset_ref = "{dataset_ref}"
backend = "peft"
recipe_hash = "{plan_ref}"
exit_code = 0
run_dir = "{}"
run_path = "{}"
metrics_path = "{}"
raw_log_path = "{}"
"#,
            &run_ref[..12],
            &plan_ref[..12],
            path_string(&run_dir),
            path_string(&run_path),
            path_string(&metrics_path),
            path_string(&raw_log_path),
        ),
    )
    .expect("run toml");
    fs::write(
        &metrics_path,
        "{\"type\":\"train\",\"step\":1}\nnot-json-secret\n{\"type\":\"done\"}\n",
    )
    .expect("metrics");
    fs::write(&raw_log_path, "line1\nline2\n").expect("raw log");
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

fn write_session_fixture_with_refs(
    home: &Path,
    session_ref: &str,
    title: &str,
    default_server_ref: Option<&str>,
    adapter_ref: Option<&str>,
    message_count: usize,
    messages: Option<&[String]>,
) {
    let session_dir = home.join("sessions").join(session_ref);
    fs::create_dir_all(&session_dir).expect("session dir");
    let default_server_ref = default_server_ref
        .map(|value| format!("default_server_ref = \"{value}\"\n"))
        .unwrap_or_default();
    let adapter_ref = adapter_ref
        .map(|value| format!("adapter_ref = \"{value}\"\n"))
        .unwrap_or_default();
    fs::write(
        session_dir.join("session.toml"),
        format!(
            r#"schema = "tentgent.session.v1"
session_ref = "{session_ref}"
short_ref = "{}"
title = "{title}"
created_at = "2026-05-01T00:00:00Z"
updated_at = "2026-05-01T00:10:00Z"
message_count = {message_count}
{default_server_ref}{adapter_ref}tags = []
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

fn session_messages_n(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| session_message("user", &format!("message {index}")))
        .collect()
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

struct MockChatResponse {
    status: u16,
    content_type: &'static str,
    response_body: &'static [u8],
    include_content_length: bool,
}

async fn spawn_mock_chat_server_sequence(
    responses: Vec<MockChatResponse>,
) -> (u16, oneshot::Receiver<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("listener");
    let port = listener.local_addr().expect("local addr").port();
    let (sender, receiver) = oneshot::channel();
    tokio::spawn(async move {
        let mut bodies = Vec::new();
        for response_config in responses {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let body = read_mock_request_body(&mut stream).await;
            bodies.push(body);
            write_mock_response(&mut stream, response_config).await;
        }
        let _ = sender.send(bodies);
    });

    (port, receiver)
}

async fn read_mock_request_body(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
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
    body
}

async fn write_mock_response(
    stream: &mut tokio::net::TcpStream,
    response_config: MockChatResponse,
) {
    let length_header = if response_config.include_content_length {
        format!(
            "Content-Length: {}\r\n",
            response_config.response_body.len()
        )
    } else {
        String::new()
    };
    let cache_header = if response_config
        .content_type
        .starts_with("text/event-stream")
    {
        "Cache-Control: no-cache\r\n"
    } else {
        ""
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n{}{}Connection: close\r\n\r\n",
        response_config.status,
        reason_phrase(response_config.status),
        response_config.content_type,
        cache_header,
        length_header
    );
    stream
        .write_all(response.as_bytes())
        .await
        .expect("write headers");
    stream
        .write_all(response_config.response_body)
        .await
        .expect("write body");
    stream.shutdown().await.expect("shutdown");
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
