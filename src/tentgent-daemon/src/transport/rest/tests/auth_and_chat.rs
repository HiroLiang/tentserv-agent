use super::*;

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
async fn daemon_token_protects_v1_routes_but_not_healthz() {
    let requested_home = unique_home("daemon-token");
    let state = rest_state_for_home_with_security(
        requested_home,
        DaemonSecurityConfig::from_token_value(Some("secret")),
    );
    let home = state.app().layout().home_dir.canonicalize().expect("home");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/status")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer")
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/status")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn auth_status_lists_gemini_provider() {
    let state = rest_state("auth-status");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/auth")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let providers = body["providers"].as_array().expect("providers");
    assert!(providers
        .iter()
        .any(|provider| provider["provider"] == "gemini"));
    assert!(providers
        .iter()
        .all(|provider| provider["source_mode"] == "auto"));
}

#[tokio::test]
async fn auth_provider_rejects_invalid_provider() {
    let state = rest_state("auth-invalid-provider");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/auth/not-real")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("gemini"));
}

#[tokio::test]
async fn doctor_returns_observational_report() {
    let home = unique_home("doctor-report");
    let state = rest_state_for_home(home.clone());
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/doctor")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(matches!(
        body["status"].as_str(),
        Some("pass" | "warn" | "fail")
    ));
    assert!(body["summary"].is_object());
    assert!(body["checks"].is_array());
    assert!(body["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| check["category"] == "runtime-ownership"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn doctor_includes_cluster_readiness_summary() {
    let requested_home = unique_home("doctor-cluster-readiness");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "6".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);

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
                .uri("/v1/doctor")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let cluster_check = body["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["category"] == "cluster")
        .expect("cluster doctor check");
    assert_eq!(cluster_check["name"], "cluster readiness");
    assert_eq!(cluster_check["status"], "warn");
    assert_eq!(
        cluster_check["description"],
        "1/1 cluster(s) need attention"
    );
    assert!(cluster_check["flags"]
        .as_array()
        .expect("flags")
        .iter()
        .any(|flag| flag == "has-cluster-attention"));
    assert_eq!(cluster_check["next_actions"][0]["code"], "inspect-cluster");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn daemon_logs_return_metadata() {
    let state = rest_state("daemon-logs");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/daemon/logs")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["logs"]["stdout"]["kind"], "stdout");
    assert_eq!(body["logs"]["stderr"]["kind"], "stderr");
}

#[tokio::test]
async fn daemon_log_content_reads_tail() {
    let home = unique_home("daemon-log-tail");
    let state = rest_state_for_home(home.clone());
    fs::create_dir_all(home.join("logs")).expect("logs dir");
    fs::write(home.join("logs/daemon.stderr.log"), "abcdef").expect("daemon stderr log");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/daemon/logs/stderr?tail_bytes=4")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["log"]["owner"], "daemon");
    assert_eq!(body["log"]["server_ref"], Value::Null);
    assert_eq!(body["log"]["kind"], "stderr");
    assert_eq!(body["log"]["tail_bytes"], 4);
    assert_eq!(body["log"]["content"], "cdef");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn daemon_log_content_rejects_invalid_tail_bytes() {
    let state = rest_state("daemon-log-invalid-tail");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/daemon/logs/stdout?tail_bytes=0")
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
async fn chat_stream_returns_sse_error_for_runtime_failures() {
    let state = rest_state("chat-stream");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"aaaaaaaaaaaa","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream")));
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8");
    assert!(body.contains("event: error"));
    assert!(body.contains("chat_model_failed"));
}

#[tokio::test]
async fn chat_rejects_invalid_message_role() {
    let state = rest_state("chat-invalid-role");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"aaaaaaaaaaaa","messages":[{"role":"tool","content":"hi"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn claude_messages_stream_uses_anthropic_sse_shape() {
    let state = rest_state("claude-messages-stream");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"aaaaaaaaaaaa","max_tokens":12,"messages":[{"role":"user","content":"hi"}],"stream":true}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = sse_body(response).await;
    assert!(body.contains("event: message_start"));
    assert!(body.contains("event: content_block_start"));
    assert!(body.contains("event: error"));
    assert!(body.contains(r#""type":"chat_model_failed""#));
}

#[tokio::test]
async fn gemini_stream_generate_content_uses_gemini_sse_shape() {
    let state = rest_state("gemini-stream");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/aaaaaaaaaaaa:streamGenerateContent?alt=sse")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = sse_body(response).await;
    assert!(body.contains(r#""error":{"code":"chat_model_failed""#));
    assert!(!body.contains("event:"));
}

#[tokio::test]
async fn gemini_generate_content_rejects_non_text_parts() {
    let state = rest_state("gemini-non-text");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1beta/models/aaaaaaaaaaaa:generateContent")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"contents":[{"role":"user","parts":[{"inlineData":{"mimeType":"text/plain","data":"aGk="}}]}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "unsupported_provider_content");
}

#[tokio::test]
async fn chat_family_routes_reject_non_chat_models_before_runtime() {
    for (label, capability, model_ref) in [
        ("embedding", "embedding", "4".repeat(64)),
        ("rerank", "rerank", "5".repeat(64)),
    ] {
        let requested_home = unique_home(&format!("chat-route-{label}"));
        let state = rest_state_for_home(requested_home);
        let home = state.app().layout().home_dir.canonicalize().expect("home");
        write_model_fixture_with_capabilities(&home, &model_ref, &[capability]);

        let requests = [
            (
                "/v1/chat".to_string(),
                format!(
                    r#"{{"model_ref":"{model_ref}","messages":[{{"role":"user","content":"hi"}}]}}"#
                ),
            ),
            (
                "/v1/chat/completions".to_string(),
                format!(
                    r#"{{"model":"{model_ref}","messages":[{{"role":"user","content":"hi"}}]}}"#
                ),
            ),
            (
                "/v1/messages".to_string(),
                format!(
                    r#"{{"model":"{model_ref}","max_tokens":12,"messages":[{{"role":"user","content":"hi"}}]}}"#
                ),
            ),
            (
                format!("/v1beta/models/{model_ref}:generateContent"),
                r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#.to_string(),
            ),
        ];

        for (uri, body) in requests {
            let response = build_router(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(uri)
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .expect("request"),
                )
                .await
                .expect("response");

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = json_body(response).await;
            assert_eq!(body["error"], "unsupported_target");
            let message = body["message"].as_str().expect("message");
            assert!(message.contains("chat endpoint"));
            assert!(message.contains("requires model capability `chat`"));
            assert!(message.contains(capability));
        }

        let _ = fs::remove_dir_all(home);
    }
}

#[tokio::test]
async fn embeddings_reject_empty_input_without_session_writes() {
    let requested_home = unique_home("embeddings-empty-input");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"model_ref":"aaaaaaaaaaaa","input":[]}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(fs::read_dir(home.join("sessions"))
        .expect("sessions dir")
        .next()
        .is_none());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn embeddings_reject_non_embedding_models_before_runtime() {
    for (label, capability, model_ref) in [
        ("chat", "chat", "8".repeat(64)),
        ("rerank", "rerank", "9".repeat(64)),
    ] {
        let requested_home = unique_home(&format!("embedding-route-{label}"));
        let state = rest_state_for_home(requested_home);
        let home = state.app().layout().home_dir.canonicalize().expect("home");
        write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &[capability]);

        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/embeddings")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"model_ref":"{model_ref}","input":"hi"}}"#
                    )))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_body(response).await;
        assert_eq!(body["error"], "unsupported_target");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("embedding endpoint"));
        assert!(message.contains("requires model capability `embedding`"));
        assert!(message.contains(capability));

        let _ = fs::remove_dir_all(home);
    }
}

#[tokio::test]
async fn rerank_rejects_invalid_input_without_session_writes() {
    let requested_home = unique_home("rerank-invalid-input");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/rerank")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"aaaaaaaaaaaa","query":"q","documents":[]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(fs::read_dir(home.join("sessions"))
        .expect("sessions dir")
        .next()
        .is_none());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn rerank_rejects_non_rerank_models_before_runtime() {
    for (label, capability, model_ref) in [
        ("chat", "chat", "6".repeat(64)),
        ("embedding", "embedding", "7".repeat(64)),
    ] {
        let requested_home = unique_home(&format!("rerank-route-{label}"));
        let state = rest_state_for_home(requested_home);
        let home = state.app().layout().home_dir.canonicalize().expect("home");
        write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &[capability]);

        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/rerank")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"model_ref":"{model_ref}","query":"q","documents":["doc"]}}"#
                    )))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_body(response).await;
        assert_eq!(body["error"], "unsupported_target");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("rerank endpoint"));
        assert!(message.contains("requires model capability `rerank`"));
        assert!(message.contains(capability));

        let _ = fs::remove_dir_all(home);
    }
}

#[tokio::test]
async fn audio_transcription_job_rejects_relative_input_path() {
    let state = rest_state("audio-transcription-relative-path");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/transcriptions/jobs")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"aaaaaaaaaaaa","path":"audio.wav"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("absolute audio file path"));
}

#[tokio::test]
async fn audio_transcription_job_accepts_path_request() {
    let requested_home = unique_home("audio-transcription-job");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "c".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["audio-transcription"]);
    let input_path = home.join("fixtures/audio.wav");
    fs::create_dir_all(input_path.parent().expect("parent")).expect("fixture dir");
    fs::write(&input_path, b"not real audio").expect("audio fixture");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/transcriptions/jobs")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "model_ref": model_ref,
                        "path": path_string(&input_path),
                        "language": "en",
                        "output_format": "vtt",
                        "output_filename": "custom.vtt",
                        "timestamps": true
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
    assert_eq!(body["job"]["kind"], "audio_transcription");
    assert!(matches!(
        body["job"]["status"].as_str(),
        Some("queued" | "running")
    ));
    assert_eq!(body["job"]["target"]["section"], "audio");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    assert_eq!(
        body["job"]["target"]["path"].as_str(),
        Some(path_string(input_path).as_str())
    );

    for _ in 0..50 {
        let Some(job) = state
            .app()
            .jobs()
            .get(&crate::runtime::JobId::new(job_id.clone()))
        else {
            break;
        };
        if job.status.is_terminal() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn audio_transcription_job_accepts_multipart_upload_request() {
    let requested_home = unique_home("audio-transcription-upload-job");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "e".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["audio-transcription"]);
    let boundary = "tentgent-audio-upload-boundary";
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("language", "en"),
            MultipartPart::text("output_format", "text"),
            MultipartPart::text("output_filename", "uploaded.txt"),
            MultipartPart::text("timestamps", "false"),
            MultipartPart::file(
                "file",
                "input file.mp3",
                "audio/mpeg",
                b"not real mp3 bytes",
            ),
        ],
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/transcriptions/job")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    let job_id = body["job"]["job_id"].as_str().expect("job id").to_string();
    assert_eq!(body["job"]["kind"], "audio_transcription");
    assert!(matches!(
        body["job"]["status"].as_str(),
        Some("queued" | "running")
    ));
    assert_eq!(body["job"]["target"]["section"], "audio");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    assert!(body["job"]["target"]["path"]
        .as_str()
        .expect("target path")
        .ends_with("/input/input_file.mp3"));
    assert_eq!(body["job"]["workspace"]["input"]["state"], "done");
    assert_eq!(body["job"]["workspace"]["input"]["done"], true);
    assert_eq!(body["job"]["workspace"]["input"]["chunk_count"], 1);
    assert_eq!(
        body["job"]["workspace"]["input"]["original_filename"],
        "input file.mp3"
    );
    assert_eq!(
        body["job"]["workspace"]["input"]["media_type"],
        "audio/mpeg"
    );

    for _ in 0..50 {
        let Some(job) = state
            .app()
            .jobs()
            .get(&crate::runtime::JobId::new(job_id.clone()))
        else {
            break;
        };
        if job.status.is_terminal() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn audio_transcription_upload_job_rejects_missing_file() {
    let requested_home = unique_home("audio-transcription-upload-missing-file");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "f".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["audio-transcription"]);
    let boundary = "tentgent-audio-upload-missing-file-boundary";
    let body = multipart_body(boundary, &[MultipartPart::text("model_ref", &model_ref)]);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/transcriptions/job")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("`file` is required"));

    let _ = fs::remove_dir_all(home);
}
