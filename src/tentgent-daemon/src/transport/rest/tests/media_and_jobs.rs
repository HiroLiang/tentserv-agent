use super::*;

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
