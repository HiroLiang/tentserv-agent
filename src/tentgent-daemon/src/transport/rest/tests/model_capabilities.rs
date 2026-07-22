use super::*;

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
async fn jobs_cancel_marks_active_job_canceled() {
    let requested_home = unique_home("jobs-cancel");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state
        .app()
        .jobs()
        .create(JobKind::model_pull(), "Pull model", None, Vec::new());
    state.app().jobs().start(&job.job_id, "downloading");

    let response = build_router(state.clone())
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
    assert_eq!(body["job"]["cancellable"], true);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/jobs/{}/cancel", job.job_id.as_str()))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["job"]["status"], "canceled");
    assert_eq!(body["job"]["stage"], "canceled");
    assert_eq!(body["job"]["cancellable"], false);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn jobs_delete_rejects_active_and_removes_terminal_job() {
    let requested_home = unique_home("jobs-delete");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let active = state
        .app()
        .jobs()
        .create(JobKind::model_pull(), "Pull model", None, Vec::new());
    let terminal =
        state
            .app()
            .jobs()
            .create(JobKind::adapter_pull(), "Pull adapter", None, Vec::new());
    let workspace_store =
        FileJobWorkspaceStore::from_runtime_dir(&state.app().layout().runtime_dir);
    let terminal_workspace = workspace_store
        .open_workspace(&terminal.job_id)
        .expect("open terminal workspace");
    fs::write(
        terminal_workspace.workspace_dir.join("marker.txt"),
        b"retained",
    )
    .expect("write terminal workspace marker");
    state
        .app()
        .jobs()
        .succeed(&terminal.job_id, None, "adapter imported");
    assert!(terminal_workspace.workspace_dir.exists());

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/jobs/{}", active.job_id.as_str()))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/jobs/{}", terminal.job_id.as_str()))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["job"]["job_id"], terminal.job_id.as_str());
    assert_eq!(body["job"]["workspace"]["cleanup_state"], "removed");
    assert!(!terminal_workspace.workspace_dir.exists());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/jobs/{}", terminal.job_id.as_str()))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn daemon_shutdown_interrupts_active_jobs_and_retains_fresh_workspaces() {
    let requested_home = unique_home("daemon-shutdown-jobs");
    let state = rest_state_for_home_with_security(
        requested_home,
        DaemonSecurityConfig::from_token_value(Some("secret")),
    );
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::audio_transcription(),
        "Transcribe audio",
        None,
        Vec::new(),
    );
    let workspace_store =
        FileJobWorkspaceStore::from_runtime_dir(&state.app().layout().runtime_dir);
    let workspace = workspace_store
        .open_workspace(&job.job_id)
        .expect("open active workspace");
    fs::write(workspace.workspace_dir.join("input.wav"), b"audio")
        .expect("write active workspace marker");
    state.app().jobs().start(&job.job_id, "running");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/daemon/shutdown")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let interrupted = state.app().jobs().get(&job.job_id).expect("job");
    assert_eq!(interrupted.status, JobStatus::Interrupted);
    assert_eq!(
        interrupted.error_summary.as_deref(),
        Some("daemon shutdown requested")
    );
    assert!(workspace.workspace_dir.exists());

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
async fn sync_store_routes_validate_requests_before_mutation() {
    let requested_home = unique_home("sync-store-route-validation");
    let state = rest_state_for_home(requested_home.clone());
    let home = state.app().layout().home_dir.canonicalize().expect("home");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/models/pull")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"repo_id":"https://huggingface.co/a/b"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/adapters/pull")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"repo_id":"org/adapter","target_capability":"image-generation","adapter_format":"image-lora"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/adapters/pull/jobs")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"repo_id":"org/adapter","target_capability":"image-generation","adapter_format":"diffusers-lora","recommended_scale":8.0}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/models/pull")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"repo_id":"org/model","capability":"audio"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/v1/models/abc123")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"capability":"audio"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/adapters/pull")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"repo_id":"https://huggingface.co/a/b"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/import")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"path":"relative"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/adapters/not-hex/bind")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"base_model_ref":"aaaaaaaaaaaa"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

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
async fn model_import_route_returns_default_capability_warning() {
    let requested_home = unique_home("model-import-capability-warning");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/default-model");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("model.gguf"), b"default model").expect("source model");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/models/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "path": path_string(&source_dir)
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["chat"])
    );
    assert_eq!(body["model"]["model_capability_source"], "default-chat");
    assert_eq!(
        body["warnings"][0],
        "capability defaulted to chat; provide capability to classify another endpoint family"
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_import_route_stores_explicit_capability_metadata() {
    let requested_home = unique_home("model-import-capability");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/embedding-model");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("model.gguf"), b"embedding model").expect("source model");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/models/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "path": path_string(&source_dir),
                        "capability": "embedding"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["embedding"])
    );
    assert_eq!(body["model"]["model_capability_source"], "explicit-user");
    assert_eq!(body["mutation"]["deduplicated"], false);

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
    assert_eq!(
        body["models"][0]["model_capabilities"],
        serde_json::json!(["embedding"])
    );
    assert_eq!(
        body["models"][0]["model_capability_source"],
        "explicit-user"
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_import_job_records_default_capability_warning() {
    let requested_home = unique_home("model-import-job-capability-warning");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/default-job-model");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("model.gguf"), b"default job model").expect("source model");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/models/import/jobs")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "path": path_string(&source_dir)
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    let job_id = body["job"]["job_id"].as_str().expect("job id").to_string();

    for _ in 0..50 {
        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/jobs/{job_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let body = json_body(response).await;
        if body["job"]["status"] == "succeeded" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/jobs/{job_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["job"]["status"], "succeeded");
    assert_eq!(
        body["job"]["warning_summary"],
        "capability defaulted to chat; provide capability to classify another endpoint family"
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_import_job_preserves_explicit_capability_metadata() {
    let requested_home = unique_home("model-import-job-capability");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/rerank-model");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("model.gguf"), b"rerank model").expect("source model");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/models/import/jobs")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "path": path_string(&source_dir),
                        "capability": "rerank"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    let job_id = body["job"]["job_id"].as_str().expect("job id").to_string();

    for _ in 0..50 {
        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/jobs/{job_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let body = json_body(response).await;
        if body["job"]["status"] == "succeeded" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/jobs/{job_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["job"]["status"], "succeeded");
    assert_eq!(body["job"]["artifact"]["kind"], "model");

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
    assert_eq!(
        body["models"][0]["model_capabilities"],
        serde_json::json!(["rerank"])
    );
    assert_eq!(
        body["models"][0]["model_capability_source"],
        "explicit-user"
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capabilities_post_sets_capability_metadata() {
    let requested_home = unique_home("models-capability-post-set");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "9".repeat(64);
    write_model_fixture(&home, &model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/models/{}/capabilities", &model_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"set":["vision-chat"]}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["mutation"]["kind"], "update_capabilities");
    assert_eq!(
        body["mutation"]["previous_capabilities"],
        serde_json::json!(["chat", "embedding"])
    );
    assert_eq!(
        body["mutation"]["added"],
        serde_json::json!(["vision-chat"])
    );
    assert_eq!(
        body["mutation"]["removed"],
        serde_json::json!(["chat", "embedding"])
    );
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["vision-chat"])
    );
    assert_eq!(body["model"]["model_capability_source"], "manual-update");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["vision-chat"])
    );
    assert_eq!(body["model"]["model_capability_source"], "manual-update");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capabilities_post_adds_and_removes_capability_metadata() {
    let requested_home = unique_home("models-capability-post-add-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "8".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/models/{}/capabilities", &model_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"add":["vision-chat","embedding"],"remove":["chat"]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["embedding", "vision-chat"])
    );
    assert_eq!(
        body["mutation"]["added"],
        serde_json::json!(["embedding", "vision-chat"])
    );
    assert_eq!(body["mutation"]["removed"], serde_json::json!(["chat"]));
    assert_eq!(body["model"]["model_capability_source"], "manual-update");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["embedding", "vision-chat"])
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capabilities_post_rejects_removing_cluster_route_capability() {
    let requested_home = unique_home("models-capability-cluster-blocker");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "4".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["chat", "embedding"]);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/v1/clusters/local-assistant")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{
                        "schema_version": 1,
                        "cluster_ref": "local-assistant",
                        "routes": {{
                            "chat": {{
                                "kind": "local-model",
                                "model_ref": "{model_ref}"
                            }}
                        }}
                    }}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/models/{}/capabilities", &model_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"set":["embedding"]}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "capability_in_use");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("cluster-route local-assistant:chat"));
    assert_eq!(body["blockers"][0]["kind"], "cluster-route");
    assert_eq!(body["blockers"][0]["code"], "capability-in-use");
    assert_eq!(body["blockers"][0]["route"], "chat");

    let _ = fs::remove_dir_all(home);
}
