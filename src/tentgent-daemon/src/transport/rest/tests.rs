use std::{fs, path::PathBuf};

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use serde_json::Value;
use tentgent_kernel::{
    features::job::{
        domain::{JobResultFile, JobWorkspaceStreamSummary},
        infra::FileJobWorkspaceStore,
        ports::{JobChunkPort, JobChunkWrite, JobResultPort, JobStreamKind, JobWorkspacePort},
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};
use tower::ServiceExt;

use crate::{
    app::{DaemonAppState, DaemonServices},
    bootstrap::{DaemonBootstrapConfig, LoggingConfig, LoggingRuntime, RestConfig},
    runtime::{
        JobArtifact, JobKind, JobOutputLine, JobProgressPatch, JobProgressUpdate, JobStream,
        JobTarget,
    },
    transport::rest::{build_router, security::DaemonSecurityConfig, state::RestState},
};

mod claude_messages_compat;
mod openai_chat_compat;
mod openai_embeddings_compat;
mod openai_image_generation_compat;

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

#[tokio::test]
async fn audio_transcription_upload_job_validates_multipart_fields() {
    for (label, parts, expected_message) in [
        (
            "blank-model-ref",
            vec![
                MultipartPart::text("model_ref", "   "),
                MultipartPart::file("file", "audio.wav", "audio/wav", b"audio bytes"),
            ],
            "`model_ref` is required",
        ),
        (
            "bad-output-format",
            vec![
                MultipartPart::text("model_ref", "a".repeat(64).as_str()),
                MultipartPart::text("output_format", "docx"),
                MultipartPart::file("file", "audio.wav", "audio/wav", b"audio bytes"),
            ],
            "unsupported audio transcription output format",
        ),
        (
            "duplicate-file",
            vec![
                MultipartPart::text("model_ref", "a".repeat(64).as_str()),
                MultipartPart::file("file", "audio-one.wav", "audio/wav", b"one"),
                MultipartPart::file("file", "audio-two.wav", "audio/wav", b"two"),
            ],
            "`file` must appear exactly once",
        ),
        (
            "duplicate-model-ref",
            vec![
                MultipartPart::text("model_ref", "a".repeat(64).as_str()),
                MultipartPart::text("model_ref", "b".repeat(64).as_str()),
                MultipartPart::file("file", "audio.wav", "audio/wav", b"audio bytes"),
            ],
            "`model_ref` must not be provided more than once",
        ),
    ] {
        let requested_home = unique_home(&format!("audio-transcription-upload-{label}"));
        let state = rest_state_for_home(requested_home);
        let home = state.app().layout().home_dir.canonicalize().expect("home");
        write_safetensors_model_fixture_with_capabilities(
            &home,
            &"a".repeat(64),
            &["audio-transcription"],
        );
        let boundary = format!("tentgent-audio-upload-{label}");
        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audio/transcriptions/job")
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(multipart_body(&boundary, &parts)))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{label}");
        let body = json_body(response).await;
        assert_eq!(body["error"], "bad_request", "{label}");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains(expected_message),
            "{label}: {body}"
        );

        let _ = fs::remove_dir_all(home);
    }
}

#[tokio::test]
async fn audio_transcription_result_reports_pending_for_active_job() {
    let requested_home = unique_home("audio-transcription-result-pending");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::audio_transcription(),
        "transcribe fixture",
        None,
        Vec::<String>::new(),
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/audio/transcriptions/job/{}/result",
                    job.job_id
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "result_pending");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn audio_transcription_result_reports_failed_terminal_job() {
    let requested_home = unique_home("audio-transcription-result-failed");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::audio_transcription(),
        "transcribe fixture",
        None,
        Vec::<String>::new(),
    );
    state.app().jobs().fail(&job.job_id, "audio runtime failed");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/audio/transcriptions/job/{}/result",
                    job.job_id
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "job_failed");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn audio_transcription_result_reads_workspace_chunks() {
    let requested_home = unique_home("audio-transcription-result");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::audio_transcription(),
        "transcribe fixture",
        None,
        Vec::<String>::new(),
    );
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    store.open_workspace(&job.job_id).expect("workspace");
    store
        .write_chunk(
            &job.job_id,
            JobChunkWrite {
                stream: JobStreamKind::Result,
                index: 0,
                bytes: b"hello transcript\n".to_vec(),
            },
        )
        .expect("write chunk");
    store
        .commit_chunk(&job.job_id, JobStreamKind::Result, 0)
        .expect("commit chunk");
    let workspace = store
        .finalize_stream(
            &job.job_id,
            JobStreamKind::Result,
            JobWorkspaceStreamSummary {
                state: "done".to_string(),
                done: true,
                failed: false,
                chunk_count: 1,
                total_bytes: 17,
                sha256: None,
                media_type: Some("text/plain".to_string()),
                original_filename: Some("transcript.txt".to_string()),
            },
        )
        .expect("finalize result");
    store
        .declare_result_file(
            &job.job_id,
            JobResultFile {
                file_id: "transcript.txt".to_string(),
                filename: "transcript.txt".to_string(),
                media_type: Some("text/plain".to_string()),
                format: Some("text".to_string()),
                total_bytes: 17,
            },
        )
        .expect("declare result");
    state.app().jobs().update_workspace(&job.job_id, workspace);
    state.app().jobs().succeed(
        &job.job_id,
        None,
        "audio transcription wrote transcript.txt",
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/audio/transcriptions/job/{}/result?cursor=0&max_chunks=1",
                    job.job_id
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain")
    );
    assert_eq!(
        response
            .headers()
            .get("x-tentgent-result-done")
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    let body = sse_body(response).await;
    assert_eq!(body, "hello transcript\n");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn audio_speech_job_accepts_json_request() {
    let requested_home = unique_home("audio-speech-job");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "d".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["audio-speech"]);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/speech/job")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "model_ref": model_ref,
                        "text": "hello from tentgent",
                        "output_format": "wav",
                        "output_filename": "speech.wav",
                        "language": "en",
                        "voice": "default"
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
    assert_eq!(body["job"]["kind"], "audio_speech");
    assert!(matches!(
        body["job"]["status"].as_str(),
        Some("queued" | "running")
    ));
    assert_eq!(body["job"]["target"]["section"], "audio");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    assert!(body["job"]["target"]["path"].is_null());

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
async fn audio_speech_job_rejects_empty_text_and_invalid_output_filename() {
    let state = rest_state("audio-speech-invalid");
    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/speech/job")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"aaaaaaaaaaaa","text":"   ","output_filename":"speech.wav"}"#,
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
        .contains("must not be empty"));

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/audio/speech/job")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"aaaaaaaaaaaa","text":"hello","output_filename":"nested/speech.wav"}"#,
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
        .contains("file name, not a path"));
}

#[tokio::test]
async fn audio_speech_result_reports_pending_for_active_job() {
    let requested_home = unique_home("audio-speech-result-pending");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::audio_speech(),
        "synthesize speech",
        None,
        Vec::<String>::new(),
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/audio/speech/job/{}/result", job.job_id))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "result_pending");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn audio_speech_result_reads_workspace_chunks() {
    let requested_home = unique_home("audio-speech-result");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::audio_speech(),
        "synthesize speech",
        None,
        Vec::<String>::new(),
    );
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    store.open_workspace(&job.job_id).expect("workspace");
    store
        .write_chunk(
            &job.job_id,
            JobChunkWrite {
                stream: JobStreamKind::Result,
                index: 0,
                bytes: b"RIFFfakeWAVE".to_vec(),
            },
        )
        .expect("write chunk");
    store
        .commit_chunk(&job.job_id, JobStreamKind::Result, 0)
        .expect("commit chunk");
    let workspace = store
        .finalize_stream(
            &job.job_id,
            JobStreamKind::Result,
            JobWorkspaceStreamSummary {
                state: "done".to_string(),
                done: true,
                failed: false,
                chunk_count: 1,
                total_bytes: 12,
                sha256: None,
                media_type: Some("audio/wav".to_string()),
                original_filename: Some("speech.wav".to_string()),
            },
        )
        .expect("finalize result");
    store
        .declare_result_file(
            &job.job_id,
            JobResultFile {
                file_id: "speech.wav".to_string(),
                filename: "speech.wav".to_string(),
                media_type: Some("audio/wav".to_string()),
                format: Some("wav".to_string()),
                total_bytes: 12,
            },
        )
        .expect("declare result");
    state.app().jobs().update_workspace(&job.job_id, workspace);
    state
        .app()
        .jobs()
        .succeed(&job.job_id, None, "audio speech wrote speech.wav");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/audio/speech/job/{}/result?cursor=0&max_chunks=1",
                    job.job_id
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("audio/wav")
    );
    assert_eq!(
        response
            .headers()
            .get("x-tentgent-result-done")
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
    let body = sse_body(response).await;
    assert_eq!(body.as_bytes(), b"RIFFfakeWAVE");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_generation_files_report_pending_for_active_job() {
    let requested_home = unique_home("image-generation-result-pending");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::image_generation(),
        "generate fixture",
        None,
        Vec::<String>::new(),
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/images/generations/job/{}/files", job.job_id))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "result_pending");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_generation_result_file_lists_and_downloads_workspace_file() {
    let requested_home = unique_home("image-generation-result-file");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state.app().jobs().create(
        JobKind::image_generation(),
        "generate fixture",
        None,
        Vec::<String>::new(),
    );
    let store = FileJobWorkspaceStore::from_runtime_dir(state.app().layout().runtime_dir.clone());
    let workspace = store.open_workspace(&job.job_id).expect("workspace");
    let files_dir = workspace.workspace_dir.join("files");
    fs::create_dir_all(&files_dir).expect("files dir");
    fs::write(files_dir.join("image.png"), b"png-bytes").expect("result file");
    let workspace_summary = store
        .finalize_stream(
            &job.job_id,
            JobStreamKind::Result,
            JobWorkspaceStreamSummary {
                state: "done".to_string(),
                done: true,
                failed: false,
                chunk_count: 1,
                total_bytes: 9,
                sha256: None,
                media_type: Some("image/png".to_string()),
                original_filename: Some("image.png".to_string()),
            },
        )
        .expect("finalize result");
    store
        .declare_result_file(
            &job.job_id,
            JobResultFile {
                file_id: "image.png".to_string(),
                filename: "image.png".to_string(),
                media_type: Some("image/png".to_string()),
                format: Some("png".to_string()),
                total_bytes: 9,
            },
        )
        .expect("declare result");
    state
        .app()
        .jobs()
        .update_workspace(&job.job_id, workspace_summary);
    state
        .app()
        .jobs()
        .succeed(&job.job_id, None, "image generation wrote image.png");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/images/generations/job/{}/files", job.job_id))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["files"][0]["file_id"], "image.png");
    assert_eq!(body["files"][0]["media_type"], "image/png");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/images/generations/job/{}/files/image.png",
                    job.job_id
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let body = sse_body(response).await;
    assert_eq!(body.as_bytes(), b"png-bytes");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_transform_job_accepts_multipart_upload_request() {
    let requested_home = unique_home("image-transform-upload");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let boundary = "tentgent-image-transform";
    let model_ref = "a".repeat(64);
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::file("image", "input.png", "image/png", b"png-bytes"),
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("prompt", "make it watercolor"),
            MultipartPart::text("strength", "0.7"),
            MultipartPart::text("output_filename", "transform.png"),
        ],
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/images/transforms/job")
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    let job_id = body["job"]["job_id"].as_str().expect("job id");
    assert_eq!(body["job"]["kind"], "image_generation");
    assert_eq!(body["job"]["target"]["section"], "image");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    let input_path = state
        .app()
        .layout()
        .runtime_dir
        .join("jobs")
        .join(job_id)
        .join("workspace")
        .join("input")
        .join("input.png");
    assert_eq!(fs::read(input_path).expect("input image"), b"png-bytes");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_transform_job_validates_multipart_fields() {
    let requested_home = unique_home("image-transform-validation");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let boundary = "tentgent-image-transform-validation";
    let model_ref = "b".repeat(64);
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::file("image", "input.png", "image/png", b"png-bytes"),
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("prompt", "make it watercolor"),
            MultipartPart::text("strength", "1.5"),
        ],
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/images/transforms/job")
                .header(
                    CONTENT_TYPE,
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
        .contains("strength"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_inpaint_job_accepts_image_and_mask_upload_request() {
    let requested_home = unique_home("image-inpaint-upload");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let boundary = "tentgent-image-inpaint";
    let model_ref = "c".repeat(64);
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::file("image", "input.png", "image/png", b"image-bytes"),
            MultipartPart::file("mask", "input.png", "image/png", b"mask-bytes"),
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("prompt", "paint a blue window"),
            MultipartPart::text("strength", "1.0"),
            MultipartPart::text("output_filename", "inpaint.png"),
        ],
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/images/inpaint/job")
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    let job_id = body["job"]["job_id"].as_str().expect("job id");
    assert_eq!(body["job"]["kind"], "image_generation");
    assert_eq!(body["job"]["target"]["section"], "image");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    let input_dir = state
        .app()
        .layout()
        .runtime_dir
        .join("jobs")
        .join(job_id)
        .join("workspace")
        .join("input");
    assert_eq!(
        fs::read(input_dir.join("input.png")).expect("input image"),
        b"image-bytes"
    );
    assert_eq!(
        fs::read(input_dir.join("mask-input.png")).expect("mask image"),
        b"mask-bytes"
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_inpaint_job_requires_mask_upload() {
    let requested_home = unique_home("image-inpaint-validation");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let boundary = "tentgent-image-inpaint-validation";
    let model_ref = "d".repeat(64);
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::file("image", "input.png", "image/png", b"image-bytes"),
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("prompt", "paint a blue window"),
        ],
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/images/inpaint/job")
                .header(
                    CONTENT_TYPE,
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
    assert!(body["message"].as_str().expect("message").contains("mask"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_control_job_accepts_control_image_upload_request() {
    let requested_home = unique_home("image-control-upload");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let boundary = "tentgent-image-control";
    let model_ref = "e".repeat(64);
    let control_ref = "f".repeat(64);
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::file(
                "control_image",
                "control.png",
                "image/png",
                b"control-bytes",
            ),
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("control_ref", &control_ref),
            MultipartPart::text("control_kind", "canny"),
            MultipartPart::text("control_strength", "1.2"),
            MultipartPart::text("prompt", "follow this control image"),
            MultipartPart::text("output_filename", "control.png"),
        ],
    );

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/images/control/job")
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    let job_id = body["job"]["job_id"].as_str().expect("job id");
    assert_eq!(body["job"]["kind"], "image_generation");
    assert_eq!(body["job"]["target"]["section"], "image");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    let input_path = state
        .app()
        .layout()
        .runtime_dir
        .join("jobs")
        .join(job_id)
        .join("workspace")
        .join("input")
        .join("control.png");
    assert_eq!(
        fs::read(input_path).expect("control image"),
        b"control-bytes"
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn image_control_job_requires_control_ref() {
    let requested_home = unique_home("image-control-validation");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let boundary = "tentgent-image-control-validation";
    let model_ref = "0".repeat(64);
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::file(
                "control_image",
                "control.png",
                "image/png",
                b"control-bytes",
            ),
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("prompt", "follow this control image"),
        ],
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/images/control/job")
                .header(
                    CONTENT_TYPE,
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
        .contains("control_ref"));

    let _ = fs::remove_dir_all(home);
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
async fn jobs_cancel_marks_active_job_canceled() {
    let requested_home = unique_home("jobs-cancel");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let job = state
        .app()
        .jobs()
        .create(JobKind::model_pull(), "Pull model", None, Vec::new());
    state.app().jobs().start(&job.job_id, "downloading");

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
    state
        .app()
        .jobs()
        .succeed(&terminal.job_id, None, "adapter imported");

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
async fn model_capabilities_post_rejects_empty_final_capability_set() {
    let requested_home = unique_home("models-capability-post-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "7".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/models/{}/capabilities", &model_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"remove":["chat"]}"#))
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
        .contains("must not be empty"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capability_verify_records_and_lists_proofs() {
    let requested_home = unique_home("models-capability-verify");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "5".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["vision-chat"]);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/models/{}/capabilities/verify",
                    &model_ref[..12]
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"capability":"vision-chat"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["proof"]["capability"], "vision-chat");
    assert_eq!(body["proof"]["status"], "verified");
    assert_eq!(body["proof"]["source"], "manual-probe");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/models/{}/capabilities/proofs",
                    &model_ref[..12]
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["proofs"].as_array().expect("proofs").len(), 1);
    assert_eq!(body["proofs"][0]["capability"], "vision-chat");
    assert_eq!(body["proofs"][0]["status"], "verified");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_capability_verify_records_failed_proof_for_undeclared_capability() {
    let requested_home = unique_home("models-capability-verify-failed");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "3".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["chat"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/models/{}/capabilities/verify",
                    &model_ref[..12]
                ))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"capability":"embedding"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["proof"]["capability"], "embedding");
    assert_eq!(body["proof"]["status"], "failed");
    assert!(body["proof"]["error"]
        .as_str()
        .expect("error")
        .contains("does not advertise capability"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn model_patch_capability_remains_compatibility_alias() {
    let requested_home = unique_home("models-capability-patch");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "6".repeat(64);
    write_model_fixture(&home, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"capability":"vision-chat"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_capabilities"],
        serde_json::json!(["vision-chat"])
    );

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
async fn model_remove_deletes_kernel_catalog_entry() {
    let requested_home = unique_home("models-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "d".repeat(64);
    write_model_fixture(&home, &model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/models/{}", &model_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["model"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );
    assert!(!home.join("models/store").join(&model_ref).exists());

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
async fn adapter_remove_deletes_kernel_catalog_entry() {
    let requested_home = unique_home("adapters-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let adapter_ref = "e".repeat(64);
    let base_model_ref = "f".repeat(64);
    write_adapter_fixture(&home, &adapter_ref, &base_model_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/adapters/{}", &adapter_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["adapter"]["adapter_ref"].as_str(),
        Some(adapter_ref.as_str())
    );
    assert!(!home.join("adapters/store").join(&adapter_ref).exists());

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
async fn train_plans_returns_empty_catalog_for_isolated_home() {
    let requested_home = unique_home("train-plans-empty");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/train/lora/plans")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["plans"].as_array().expect("plans").len(), 0);

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn train_plan_preview_rejects_path_like_refs_before_kernel_lookup() {
    let state = rest_state("train-plan-preview-invalid-ref");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/train/lora/plans/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model_ref":"models/local","dataset_ref":"aaaaaaaaaaaa","backend":"auto"}"#,
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
async fn dataset_deterministic_routes_validate_template_export_and_diff() {
    let requested_home = unique_home("datasets-deterministic-tools");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/source-dataset");
    fs::create_dir_all(&source_dir).expect("source dataset");
    fs::write(source_dir.join("train.jsonl"), sample_dataset_record()).expect("train jsonl");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/validate")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"path":"{}"}}"#,
                    path_string(&source_dir)
                )))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["valid"], true);
    assert_eq!(body["source"]["kind"], "path");
    assert_eq!(body["records"], 1);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/template")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"task":"chat","language":"en"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["task"], "chat");
    assert!(body["content"]
        .as_str()
        .expect("content")
        .contains("train.jsonl"));

    let first_ref = "9".repeat(64);
    let second_ref = "a".repeat(64);
    write_dataset_fixture(&home, &first_ref);
    write_dataset_fixture(&home, &second_ref);
    let manifest =
        r#"{"files":[{"relative_path":"train.jsonl","size_bytes":111,"sha256":"same"}]}"#;
    fs::write(
        home.join("datasets/store")
            .join(&first_ref)
            .join("manifest.json"),
        manifest,
    )
    .expect("first manifest");
    fs::write(
        home.join("datasets/store")
            .join(&second_ref)
            .join("manifest.json"),
        manifest,
    )
    .expect("second manifest");
    fs::write(
        home.join("datasets/store")
            .join(&first_ref)
            .join("source/train.jsonl"),
        sample_dataset_record(),
    )
    .expect("first source");
    fs::write(
        home.join("datasets/store")
            .join(&second_ref)
            .join("source/train.jsonl"),
        sample_dataset_record(),
    )
    .expect("second source");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/datasets/{}/diff", &first_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"right_dataset_ref":"{}"}}"#,
                    &second_ref[..12]
                )))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["left"]["short_ref"], &first_ref[..12]);
    assert_eq!(body["right"]["short_ref"], &second_ref[..12]);
    assert_eq!(body["diff"]["summary"]["unchanged"], 1);

    let export_dir = home.join("exports/dataset");
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/datasets/{}/export", &first_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"output_path":"{}"}}"#,
                    path_string(&export_dir)
                )))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["dataset"]["dataset_ref"].as_str(),
        Some(first_ref.as_str())
    );
    assert!(export_dir.join("train.jsonl").is_file());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn dataset_sync_import_stores_local_dataset() {
    let requested_home = unique_home("dataset-sync-import");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let source_dir = home.join("fixtures/importable-dataset");
    fs::create_dir_all(&source_dir).expect("source dataset");
    fs::write(source_dir.join("train.jsonl"), sample_dataset_record()).expect("train jsonl");

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/datasets/import")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"path":"{}"}}"#,
                    path_string(&source_dir)
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["mutation"]["kind"], "import");
    assert_eq!(body["dataset"]["tuning_ready"], true);
    let dataset_ref = body["dataset"]["dataset_ref"]
        .as_str()
        .expect("dataset ref");
    assert!(home
        .join("datasets/store")
        .join(dataset_ref)
        .join("source/train.jsonl")
        .is_file());

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
async fn dataset_remove_deletes_kernel_catalog_entry() {
    let requested_home = unique_home("datasets-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let dataset_ref = "8".repeat(64);
    write_dataset_fixture(&home, &dataset_ref);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/datasets/{}", &dataset_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["dataset"]["dataset_ref"].as_str(),
        Some(dataset_ref.as_str())
    );
    assert!(!home.join("datasets/store").join(&dataset_ref).exists());

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
    assert_eq!(servers[0]["capability"], "chat");
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
    assert_eq!(server["capability"], "chat");
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
async fn server_create_infers_capability_from_model_metadata() {
    let requested_home = unique_home("servers-create");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "b".repeat(64);
    write_model_fixture(&home, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","host":"127.0.0.1","port":8998,"lazy_load":true,"idle_seconds":30}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["created"], true);
    assert_eq!(body["server"]["runtime_kind"], "local");
    assert_eq!(body["server"]["capability"], "embedding");
    assert_eq!(
        body["server"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );
    assert_eq!(body["server"]["port"], 8998);
    assert_eq!(body["server"]["lazy_load"], true);
    assert_eq!(body["server"]["idle_seconds"], 30);
    let server_ref = body["server"]["server_ref"].as_str().expect("server ref");
    assert!(home
        .join("servers")
        .join(server_ref)
        .join("server.toml")
        .exists());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_rejects_non_chat_model_for_chat_server() {
    let requested_home = unique_home("servers-create-non-chat");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "3".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["embedding"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","capability":"chat","host":"127.0.0.1","port":8998}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "unsupported_target");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("requires model capability `chat`"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_prepares_embedding_server_spec() {
    let requested_home = unique_home("servers-create-embedding");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "5".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["embedding"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","capability":"embedding","host":"127.0.0.1","port":8998}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["server"]["capability"], "embedding");
    assert_eq!(
        body["server"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_create_prepares_rerank_server_spec() {
    let requested_home = unique_home("servers-create-rerank");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "6".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["rerank"]);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/servers")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"runtime_ref":"{model_ref}","capability":"rerank","host":"127.0.0.1","port":8999}}"#
                )))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["server"]["capability"], "rerank");
    assert_eq!(
        body["server"]["model_ref"].as_str(),
        Some(model_ref.as_str())
    );

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_remove_deletes_stopped_spec() {
    let requested_home = unique_home("servers-remove");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "c".repeat(64);
    let model_ref = "d".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/servers/{}", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["removed"]["kind"], "server");
    assert_eq!(
        body["removed"]["server_ref"].as_str(),
        Some(server_ref.as_str())
    );
    assert!(!home.join("servers").join(&server_ref).exists());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_start_returns_conflict_for_running_server() {
    let requested_home = unique_home("servers-start-running");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "d".repeat(64);
    let model_ref = "e".repeat(64);
    write_model_fixture(&home, &model_ref);
    write_server_fixture(&home, &server_ref, &model_ref);
    write_server_process_fixture(&home, &server_ref, std::process::id());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/servers/{}/start", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "already_running");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_start_rejects_incompatible_stored_chat_spec() {
    let requested_home = unique_home("servers-start-non-chat-model");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "a".repeat(64);
    let model_ref = "4".repeat(64);
    write_model_fixture_with_capabilities(&home, &model_ref, &["embedding"]);
    write_server_fixture_with_capability(&home, &server_ref, &model_ref, Some("chat"));

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/servers/{}/start", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "unsupported_target");
    assert!(body["message"]
        .as_str()
        .expect("message")
        .contains("requires model capability `chat`"));

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_stop_returns_conflict_for_stopped_server() {
    let requested_home = unique_home("servers-stop-stopped");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "e".repeat(64);
    let model_ref = "f".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/servers/{}/stop", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_running");

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_health_reports_stopped_server_without_probe() {
    let requested_home = unique_home("servers-health");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "1".repeat(64);
    let model_ref = "2".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}/health", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["server"]["server_ref"].as_str(),
        Some(server_ref.as_str())
    );
    assert_eq!(body["running"], false);
    assert_eq!(body["reachable"], false);
    assert_eq!(body["target_url"], "http://127.0.0.1:8999/healthz");
    assert!(body["target_status"].is_null());

    let _ = fs::remove_dir_all(home);
}

#[tokio::test]
async fn server_logs_return_metadata_and_tail_content() {
    let requested_home = unique_home("servers-logs");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let server_ref = "2".repeat(64);
    let model_ref = "3".repeat(64);
    write_server_fixture(&home, &server_ref, &model_ref);
    let server_dir = home.join("servers").join(&server_ref);
    fs::write(server_dir.join("stdout.log"), "hello").expect("stdout log");
    fs::write(server_dir.join("stderr.log"), "alpha-beta").expect("stderr log");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/servers/{}/logs", &server_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["logs"]["stdout"]["exists"], true);
    assert_eq!(body["logs"]["stderr"]["total_bytes"], 10);

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/servers/{}/logs/stderr?tail_bytes=4",
                    &server_ref[..12]
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["log"]["kind"], "stderr");
    assert_eq!(body["log"]["tail_bytes"], 4);
    assert_eq!(body["log"]["truncated"], true);
    assert_eq!(body["log"]["content"], "beta");

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

#[tokio::test]
async fn session_write_routes_create_update_append_compact_and_remove() {
    let requested_home = unique_home("sessions-write");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sessions")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"title":"Draft","tags":["smoke"],"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["created"], true);
    assert_eq!(body["session"]["title"], "Draft");
    assert_eq!(body["session"]["message_count"], 2);
    let session_ref = body["session"]["session_ref"]
        .as_str()
        .expect("session ref")
        .to_string();

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/sessions/{}", &session_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"Renamed","tags":["updated"]}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["session"]["title"], "Renamed");
    assert_eq!(body["session"]["tags"], serde_json::json!(["updated"]));

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{}/messages", &session_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"messages":[{"role":"user","content":"third","metadata":{"topic":"rest"}}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["session"]["message_count"], 3);
    assert_eq!(body["appended"][0]["index"], 2);
    assert_eq!(body["appended"][0]["role"], "user");

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{}/compact", &session_ref[..12]))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"keep_recent_messages":3}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["compacted"]["compacted"], false);

    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/sessions/{}", &session_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["removed"]["kind"], "session");
    assert!(!home.join("sessions").join(&session_ref).exists());

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/sessions/{}", &session_ref[..12]))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let _ = fs::remove_dir_all(home);
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json")
}

async fn sse_body(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8")
}

fn rest_state(label: &str) -> RestState {
    let home = unique_home(label);
    let state = rest_state_for_home(home.clone());
    let _ = fs::remove_dir_all(home);
    state
}

fn rest_state_for_home(home: PathBuf) -> RestState {
    rest_state_for_home_with_security(home, DaemonSecurityConfig::disabled())
}

fn rest_state_for_home_with_security(home: PathBuf, security: DaemonSecurityConfig) -> RestState {
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
    RestState::with_security(std::sync::Arc::new(state), security)
}

fn unique_home(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-daemon-rest-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}

fn write_model_fixture(home: &std::path::Path, model_ref: &str) {
    write_model_fixture_with_capabilities(home, model_ref, &["chat", "embedding"]);
}

fn write_model_fixture_with_capabilities(
    home: &std::path::Path,
    model_ref: &str,
    capabilities: &[&str],
) {
    let store_dir = home.join("models/store").join(model_ref);
    fs::create_dir_all(store_dir.join("variants/mlx/source")).expect("model source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    let capabilities = capabilities
        .iter()
        .map(|capability| format!(r#""{capability}""#))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        store_dir.join("model.toml"),
        format!(
            r#"model_ref = "{model_ref}"
short_ref = "{}"
source_kind = "local"
source_path = "{}"
primary_format = "mlx"
detected_formats = ["mlx"]
model_capabilities = [{capabilities}]
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

fn write_safetensors_model_fixture_with_capabilities(
    home: &std::path::Path,
    model_ref: &str,
    capabilities: &[&str],
) {
    let store_dir = home.join("models/store").join(model_ref);
    fs::create_dir_all(store_dir.join("variants/safetensors/source")).expect("model source dir");
    fs::write(store_dir.join("manifest.json"), "{}").expect("manifest");
    let capabilities = capabilities
        .iter()
        .map(|capability| format!(r#""{capability}""#))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        store_dir.join("model.toml"),
        format!(
            r#"model_ref = "{model_ref}"
short_ref = "{}"
source_kind = "local"
source_path = "{}"
primary_format = "safetensors"
detected_formats = ["safetensors"]
model_capabilities = [{capabilities}]
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
    write_server_fixture_with_capability(home, server_ref, model_ref, None);
}

fn write_server_fixture_with_capability(
    home: &std::path::Path,
    server_ref: &str,
    model_ref: &str,
    capability: Option<&str>,
) {
    let server_dir = home.join("servers").join(server_ref);
    fs::create_dir_all(&server_dir).expect("server dir");
    let capability = capability
        .map(|capability| format!("capability = \"{capability}\"\n"))
        .unwrap_or_default();
    fs::write(
        server_dir.join("server.toml"),
        format!(
            r#"server_ref = "{server_ref}"
short_ref = "{}"
runtime_kind = "local"
{capability}model_ref = "{model_ref}"
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

fn write_server_process_fixture(home: &std::path::Path, server_ref: &str, pid: u32) {
    fs::write(
        home.join("servers").join(server_ref).join("process.toml"),
        format!(
            r#"pid = {pid}
launch_mode = "background"
started_at = "2026-05-01T00:00:00Z"
"#
        ),
    )
    .expect("server process metadata");
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

fn sample_dataset_record() -> &'static str {
    r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi"}]}"#
}

struct MultipartPart {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

impl MultipartPart {
    fn text(name: impl Into<String>, body: impl AsRef<str>) -> Self {
        Self {
            name: name.into(),
            filename: None,
            content_type: None,
            body: body.as_ref().as_bytes().to_vec(),
        }
    }

    fn file(
        name: impl Into<String>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        body: impl AsRef<[u8]>,
    ) -> Self {
        Self {
            name: name.into(),
            filename: Some(filename.into()),
            content_type: Some(content_type.into()),
            body: body.as_ref().to_vec(),
        }
    }
}

fn multipart_body(boundary: &str, parts: &[MultipartPart]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match part.filename.as_deref() {
            Some(filename) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    part.name, filename
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n", part.name).as_bytes(),
            ),
        }
        if let Some(content_type) = part.content_type.as_deref() {
            body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&part.body);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

fn path_string(path: impl AsRef<std::path::Path>) -> String {
    path.as_ref().display().to_string()
}
