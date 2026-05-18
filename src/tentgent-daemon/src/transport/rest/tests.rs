use std::{fs, path::PathBuf};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::Value;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};
use tower::ServiceExt;

use crate::{
    app::{DaemonAppState, DaemonServices},
    bootstrap::{DaemonBootstrapConfig, LoggingConfig, LoggingRuntime, RestConfig},
    runtime::{
        JobArtifact, JobKind, JobOutputLine, JobProgressPatch, JobProgressUpdate, JobStream,
        JobTarget,
    },
    transport::rest::{build_router, state::RestState},
};

#[tokio::test]
async fn healthz_returns_service_identity() {
    let state = rest_state("healthz");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "tentgent-daemon");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn status_reads_daemon_kernel_state() {
    let state = rest_state("status");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/status")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["service"], "tentgent-daemon");
    assert_eq!(body["status"], "stopped");
    assert!(body["runtime_home"]
        .as_str()
        .expect("runtime_home")
        .contains("tentgent-daemon-rest-status"));
}

#[tokio::test]
async fn jobs_returns_empty_registry_for_isolated_home() {
    let requested_home = unique_home("jobs-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/jobs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["jobs"].as_array().expect("jobs").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn jobs_list_and_inspect_runtime_registry() {
    let requested_home = unique_home("jobs-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::model_pull(),
        "Pull model",
        Some(JobTarget::new("models").with_reference("hf/repo")),
        ["models".to_string()],
    );
    state.app().jobs().start(&job.job_id, "downloading");
    state.app().jobs().update_progress(
        &job.job_id,
        JobProgressUpdate {
            stage: Some("downloading config".to_string()),
            progress: JobProgressPatch {
                bytes_done: Some(50),
                bytes_total: Some(100),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                "downloaded config.json",
            )],
            warning_summary: Some("slow network".to_string()),
        },
    );
    state.app().jobs().succeed(
        &job.job_id,
        Some(JobArtifact::new("model").with_reference("abcdef123456")),
        "model imported",
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/jobs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let jobs = body["jobs"].as_array().expect("jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["job_id"].as_str(), Some(job.job_id.as_str()));
    assert_eq!(jobs[0]["kind"], "model_pull");
    assert_eq!(jobs[0]["status"], "succeeded");
    assert_eq!(jobs[0]["target"]["section"], "models");
    assert_eq!(jobs[0]["target"]["reference"], "hf/repo");
    assert_eq!(jobs[0]["artifact"]["reference"], "abcdef123456");
    assert_eq!(jobs[0]["progress"]["percent"], 50.0);
    assert_eq!(jobs[0]["output"]["tail"][0]["stream"], "event");
    assert_eq!(jobs[0]["warning_summary"], "slow network");
    assert_eq!(jobs[0]["result_summary"], "model imported");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/jobs/{}", job.job_id.as_str()))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["job"]["job_id"].as_str(), Some(job.job_id.as_str()));
    assert_eq!(body["job"]["status"], "succeeded");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn jobs_inspect_returns_not_found_for_missing_job() {
    let requested_home = unique_home("jobs-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/jobs/job-missing")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn jobs_reload_persisted_records_from_runtime_dir() {
    let requested_home = unique_home("jobs-reload");
    let state = rest_state_for_home(requested_home.clone());
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::adapter_pull(),
        "Pull adapter",
        None,
        ["adapters".to_string()],
    );
    state.app().jobs().succeed(
        &job.job_id,
        Some(JobArtifact::new("adapter").with_reference("fedcba654321")),
        "adapter imported",
    );

    let reloaded_state = rest_state_for_home(requested_home);
    let response = build_router(reloaded_state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/jobs/{}", job.job_id.as_str()))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["job"]["status"], "succeeded");
    assert_eq!(body["job"]["artifact"]["reference"], "fedcba654321");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_pull_job_rejects_invalid_repo_id_before_starting_job() {
    let requested_home = unique_home("model-pull-job-invalid");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/models/pull/jobs")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"repo_id":"https://huggingface.co/a/b"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn models_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("models-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["models"].as_array().expect("models").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("models-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "a".repeat(64);
    write_model_fixture(&home, &model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let models = body["models"].as_array().expect("models");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["model_ref"].as_str(), Some(model_ref.as_str()));
    assert_eq!(models[0]["short_ref"].as_str(), Some(&model_ref[..12]));
    assert_eq!(models[0]["format"], "mlx");
    assert_eq!(
        models[0]["model_capabilities"],
        serde_json::json!(["chat", "embedding"])
    );
    assert_eq!(models[0]["model_capability_source"], "explicit-user");
    assert!(models[0].get("manifest_path").is_none());
    assert!(models[0].get("variant_source_path").is_none());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let model = &body["model"];
    assert_eq!(model["model_ref"].as_str(), Some(model_ref.as_str()));
    let expected_manifest_path = path_string(
        home.join("models/store")
            .join(&model_ref)
            .join("manifest.json"),
    );
    assert_eq!(
        model["manifest_path"].as_str(),
        Some(expected_manifest_path.as_str())
    );
    let expected_variant_source_path = path_string(
        home.join("models/store")
            .join(&model_ref)
            .join("variants/mlx/source"),
    );
    assert_eq!(
        model["variant_source_path"].as_str(),
        Some(expected_variant_source_path.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("models-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "b".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("models-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "c".repeat(63));
    let second_ref = format!("{}1", "c".repeat(63));
    write_model_fixture(&home, &first_ref);
    write_model_fixture(&home, &second_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &first_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "ambiguous_ref");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_inspect_rejects_invalid_reference() {
    let state = rest_state("models-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models/not-hex")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn adapters_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("adapters-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/adapters")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["adapters"].as_array().expect("adapters").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn adapter_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("adapters-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let adapter_ref = "d".repeat(64);
    let base_model_ref = "e".repeat(64);
    write_adapter_fixture(&home, &adapter_ref, &base_model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/adapters")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let adapters = body["adapters"].as_array().expect("adapters");
    assert_eq!(adapters.len(), 1);
    assert_eq!(
        adapters[0]["adapter_ref"].as_str(),
        Some(adapter_ref.as_str())
    );
    assert_eq!(adapters[0]["short_ref"].as_str(), Some(&adapter_ref[..12]));
    assert_eq!(adapters[0]["format"], "mlx");
    assert_eq!(adapters[0]["type"], "lora");
    assert_eq!(
        adapters[0]["base_model_ref"].as_str(),
        Some(base_model_ref.as_str())
    );
    assert_eq!(adapters[0]["backend_support"], serde_json::json!(["mlx"]));
    assert!(adapters[0].get("manifest_path").is_none());
    assert!(adapters[0].get("managed_source_path").is_none());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/adapters/{}", &adapter_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let adapter = &body["adapter"];
    assert_eq!(adapter["adapter_ref"].as_str(), Some(adapter_ref.as_str()));
    let expected_manifest_path = path_string(
        home.join("adapters/store")
            .join(&adapter_ref)
            .join("manifest.json"),
    );
    assert_eq!(
        adapter["manifest_path"].as_str(),
        Some(expected_manifest_path.as_str())
    );
    let expected_managed_source_path = path_string(
        home.join("adapters/store")
            .join(&adapter_ref)
            .join("source"),
    );
    assert_eq!(
        adapter["managed_source_path"].as_str(),
        Some(expected_managed_source_path.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn adapter_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("adapters-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let adapter_ref = "f".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/adapters/{}", &adapter_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn adapter_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("adapters-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "1".repeat(63));
    let second_ref = format!("{}1", "1".repeat(63));
    let base_model_ref = "2".repeat(64);
    write_adapter_fixture(&home, &first_ref, &base_model_ref);
    write_adapter_fixture(&home, &second_ref, &base_model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/adapters/{}", &first_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "ambiguous_ref");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn adapter_inspect_rejects_invalid_reference() {
    let state = rest_state("adapters-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/adapters/not-hex")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn datasets_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("datasets-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/datasets")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["datasets"].as_array().expect("datasets").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn dataset_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("datasets-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let dataset_ref = "3".repeat(64);
    write_dataset_fixture(&home, &dataset_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/datasets")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let datasets = body["datasets"].as_array().expect("datasets");
    assert_eq!(datasets.len(), 1);
    assert_eq!(
        datasets[0]["dataset_ref"].as_str(),
        Some(dataset_ref.as_str())
    );
    assert_eq!(datasets[0]["short_ref"].as_str(), Some(&dataset_ref[..12]));
    assert_eq!(datasets[0]["format"], "directory");
    assert_eq!(datasets[0]["source_kind"], "local");
    assert_eq!(datasets[0]["tuning_ready"], true);
    assert_eq!(datasets[0]["splits"]["train"], "train.jsonl");
    assert_eq!(datasets[0]["splits"]["validation"], "valid.jsonl");
    assert_eq!(
        datasets[0]["warnings"],
        serde_json::json!(["small training set"])
    );
    assert!(datasets[0].get("manifest_path").is_none());
    assert!(datasets[0].get("managed_source_path").is_none());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/datasets/{}", &dataset_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let dataset = &body["dataset"];
    assert_eq!(dataset["dataset_ref"].as_str(), Some(dataset_ref.as_str()));
    let expected_manifest_path = path_string(
        home.join("datasets/store")
            .join(&dataset_ref)
            .join("manifest.json"),
    );
    assert_eq!(
        dataset["manifest_path"].as_str(),
        Some(expected_manifest_path.as_str())
    );
    let expected_managed_source_path = path_string(
        home.join("datasets/store")
            .join(&dataset_ref)
            .join("source"),
    );
    assert_eq!(
        dataset["managed_source_path"].as_str(),
        Some(expected_managed_source_path.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn dataset_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("datasets-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let dataset_ref = "4".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/datasets/{}", &dataset_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn dataset_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("datasets-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "5".repeat(63));
    let second_ref = format!("{}1", "5".repeat(63));
    write_dataset_fixture(&home, &first_ref);
    write_dataset_fixture(&home, &second_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/datasets/{}", &first_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "ambiguous_ref");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn dataset_inspect_rejects_invalid_reference() {
    let state = rest_state("datasets-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/datasets/not-hex")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn servers_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("servers-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/servers")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["servers"].as_array().expect("servers").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_list_and_inspect_read_kernel_catalog() {
    let requested_home = unique_home("servers-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "6".repeat(64);
    let model_ref = "7".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/servers")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let servers = body["servers"].as_array().expect("servers");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["server_ref"].as_str(), Some(server_ref.as_str()));
    assert_eq!(servers[0]["short_ref"].as_str(), Some(&server_ref[..12]));
    assert_eq!(servers[0]["runtime_kind"], "local");
    assert_eq!(servers[0]["model_ref"].as_str(), Some(model_ref.as_str()));
    assert_eq!(servers[0]["host"], "127.0.0.1");
    assert_eq!(servers[0]["port"], 8999);
    assert_eq!(servers[0]["lazy_load"], false);
    assert_eq!(servers[0]["idle_seconds"], 60);
    assert_eq!(servers[0]["running"], false);
    assert!(servers[0]["process"].is_null());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let server = &body["server"];
    assert_eq!(server["server_ref"].as_str(), Some(server_ref.as_str()));
    let expected_server_dir = path_string(home.join("servers").join(&server_ref));
    assert_eq!(
        server["server_dir"].as_str(),
        Some(expected_server_dir.as_str())
    );
    let expected_spec_path =
        path_string(home.join("servers").join(&server_ref).join("server.toml"));
    assert_eq!(
        server["spec_path"].as_str(),
        Some(expected_spec_path.as_str())
    );
    let expected_process_path =
        path_string(home.join("servers").join(&server_ref).join("process.toml"));
    assert_eq!(
        server["process_path"].as_str(),
        Some(expected_process_path.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("servers-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "8".repeat(64);
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("servers-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let first_ref = format!("{}0", "9".repeat(63));
    let second_ref = format!("{}1", "9".repeat(63));
    let model_ref = "a".repeat(64);
    write_server_fixture(&home, &first_ref, &model_ref);
    write_server_fixture(&home, &second_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}", &first_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "ambiguous_ref");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_inspect_rejects_invalid_reference() {
    let state = rest_state("servers-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/servers/not-hex")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn sessions_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("sessions-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["sessions"].as_array().expect("sessions").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_list_inspect_and_messages_read_kernel_store() {
    let requested_home = unique_home("sessions-catalog");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let older_ref = "111111111111000000000000";
    let newer_ref = "222222222222000000000000";
    write_session_fixture(
        &home,
        older_ref,
        "Older",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        0,
        None,
    );
    write_session_fixture(
        &home,
        newer_ref,
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

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let sessions = body["sessions"].as_array().expect("sessions");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["session_ref"], newer_ref);
    assert_eq!(sessions[0]["short_ref"], "222222222222");
    assert_eq!(sessions[0]["title"], "Newer");
    assert_eq!(
        sessions[0]["store_path"].as_str(),
        Some(path_string(home.join("sessions").join(newer_ref)).as_str())
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/222222")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let session = &body["session"];
    assert_eq!(session["session_ref"], newer_ref);
    let expected_messages_path =
        path_string(home.join("sessions").join(newer_ref).join("messages.jsonl"));
    assert_eq!(
        session["messages_path"].as_str(),
        Some(expected_messages_path.as_str())
    );
    assert!(session["warnings"].as_array().expect("warnings").is_empty());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/222222/messages?tail=2&ignored=true")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(body["session"]["session_ref"], newer_ref);
    assert_eq!(body["tail"], 2);
    assert_eq!(body["total_messages"], 3);
    assert_eq!(body["truncated"], true);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["index"], 1);
    assert_eq!(messages[0]["role"], "assistant");
    assert_eq!(messages[0]["metadata"], serde_json::json!({}));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_messages_reject_invalid_tail() {
    let state = rest_state("sessions-invalid-tail");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/222222222222/messages?tail=0")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn session_inspect_returns_not_found_for_missing_reference() {
    let requested_home = unique_home("sessions-not-found");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/333333333333")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_inspect_returns_conflict_for_ambiguous_prefix() {
    let requested_home = unique_home("sessions-ambiguous");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    write_session_fixture(
        &home,
        "444444444444000000000000",
        "First",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:10:00Z",
        0,
        None,
    );
    write_session_fixture(
        &home,
        "444444444444111111111111",
        "Second",
        "2026-05-01T00:00:00Z",
        "2026-05-01T00:20:00Z",
        0,
        None,
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/444444")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "ambiguous_ref");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_inspect_rejects_invalid_reference() {
    let state = rest_state("sessions-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/not-hex")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json")
}

fn rest_state(label: &str) -> RestState {
    let home = unique_home(label);
    let state = rest_state_for_home(home.clone());
    let _ = fs::remove_dir_all(home);
    state
}

fn rest_state_for_home(home: PathBuf) -> RestState {
    let config = DaemonBootstrapConfig {
        home: Some(home.clone()),
        logging: LoggingConfig {
            enabled: false,
            env_filter: None,
        },
        rest: RestConfig::default(),
    };
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: Some(home.clone()),
            data_root_dir: None,
        })
        .expect("layout");
    let services = DaemonServices::bootstrap(&config).expect("services");
    let state = DaemonAppState::new(
        services,
        LoggingRuntime::disabled(),
        layout,
        RestConfig::default(),
    );
    RestState::new(std::sync::Arc::new(state))
}

fn unique_home(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-daemon-rest-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}

fn write_model_fixture(home: &std::path::Path, model_ref: &str) {
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
model_capabilities = ["chat", "embedding"]
model_capability_source = "explicit-user"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
"#,
            &model_ref[..12],
            path_string(home.join("fixtures/model"))
        ),
    )
    .expect("model metadata");
}

fn write_adapter_fixture(home: &std::path::Path, adapter_ref: &str, base_model_ref: &str) {
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
base_model_ref = "{base_model_ref}"
base_model_source_repo = "mlx-community/base-model"
base_model_source_revision = "resolved-sha"
model_family = "qwen"
backend_support = ["mlx"]
source_kind = "local"
source_path = "{}"
training_dataset_ref = "dataset-ref"
training_run_ref = "run-ref"
training_config_ref = "config-ref"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
"#,
            &adapter_ref[..12],
            path_string(home.join("fixtures/adapter"))
        ),
    )
    .expect("adapter metadata");
}

fn write_dataset_fixture(home: &std::path::Path, dataset_ref: &str) {
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
warnings = ["small training set"]

[package.splits]
train = "train.jsonl"
validation = "valid.jsonl"
test = "test.jsonl"
eval_cases = "eval_cases.jsonl"
source_manifest = "manifest.json"
"#,
            &dataset_ref[..12],
            path_string(home.join("fixtures/dataset"))
        ),
    )
    .expect("dataset metadata");
}

fn write_server_fixture(home: &std::path::Path, server_ref: &str, model_ref: &str) {
    let server_dir = home.join("servers").join(server_ref);
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(
        server_dir.join("server.toml"),
        format!(
            r#"server_ref = "{server_ref}"
short_ref = "{}"
runtime_kind = "local"
model_ref = "{model_ref}"
host = "127.0.0.1"
port = 8999
lazy_load = false
idle_seconds = 60
created_at = "2026-05-01T00:00:00Z"
"#,
            &server_ref[..12]
        ),
    )
    .expect("server spec");
}

fn write_session_fixture(
    home: &std::path::Path,
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
default_server_ref = "server-ref"
adapter_ref = "adapter-ref"
tags = ["smoke", "rest"]
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
        r#"{{"schema":"tentgent.session.message.v1","role":"{role}","content":"{content}","created_at":"2026-05-01T00:00:00Z","metadata":{{}}}}"#
    )
}

fn path_string(path: impl AsRef<std::path::Path>) -> String {
    path.as_ref().display().to_string()
}
