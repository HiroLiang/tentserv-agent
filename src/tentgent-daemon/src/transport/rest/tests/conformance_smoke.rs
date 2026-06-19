use std::fs;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use tower::ServiceExt;

use crate::{
    runtime::JobKind,
    transport::rest::{build_router, state::RestState},
};

use super::{
    json_body, multipart_body, rest_state, rest_state_for_home, sse_body, unique_home,
    write_safetensors_model_fixture_with_capabilities, MultipartPart,
};

#[tokio::test]
async fn provider_compatible_supported_shapes_have_release_smoke_coverage() {
    let cases = [
        ProviderStreamCase {
            label: "conformance-openai-chat",
            uri: "/v1/chat/completions",
            body: r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
            expected_content: r#""object":"chat.completion.chunk""#,
            expected_done: Some("data: [DONE]"),
            forbidden_content: Some("event: error"),
        },
        ProviderStreamCase {
            label: "conformance-claude-messages",
            uri: "/v1/messages",
            body: r#"{"model":"aaaaaaaaaaaa","max_tokens":12,"messages":[{"role":"user","content":"hi"}],"stream":true}"#,
            expected_content: "event: message_start",
            expected_done: Some("event: error"),
            forbidden_content: None,
        },
        ProviderStreamCase {
            label: "conformance-gemini-stream",
            uri: "/v1beta/models/aaaaaaaaaaaa:streamGenerateContent?alt=sse",
            body: r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}],"generationConfig":{"maxOutputTokens":12}}"#,
            expected_content: r#""error":{"code":"chat_model_failed""#,
            expected_done: None,
            forbidden_content: Some("event:"),
        },
    ];

    for case in cases {
        let response = post_json(rest_state(case.label), case.uri, case.body).await;

        assert_eq!(response.status(), StatusCode::OK, "{}", case.label);
        assert_event_stream(&response, case.label);
        let body = sse_body(response).await;
        assert!(
            body.contains(case.expected_content),
            "{}: expected `{}` in `{}`",
            case.label,
            case.expected_content,
            body
        );
        if let Some(expected_done) = case.expected_done {
            assert!(
                body.contains(expected_done),
                "{}: expected `{}` in `{}`",
                case.label,
                expected_done,
                body
            );
        }
        if let Some(forbidden_content) = case.forbidden_content {
            assert!(
                !body.contains(forbidden_content),
                "{}: did not expect `{}` in `{}`",
                case.label,
                forbidden_content,
                body
            );
        }
    }
}

#[tokio::test]
async fn provider_compatible_unsupported_error_codes_are_stable() {
    let cases = [
        ProviderErrorCase {
            label: "conformance-openai-unsupported-field",
            uri: "/v1/chat/completions",
            body: r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"user","content":"hi"}],"tools":[{"type":"function","function":{"name":"lookup"}}]}"#,
            expected_code: "unsupported_provider_field",
        },
        ProviderErrorCase {
            label: "conformance-openai-unsupported-content",
            uri: "/v1/chat/completions",
            body: r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"user","content":[{"type":"text","text":"Describe this."},{"type":"image_url","image_url":{"url":"https://example.com/image.png"}}]}]}"#,
            expected_code: "unsupported_provider_content",
        },
        ProviderErrorCase {
            label: "conformance-gemini-unsupported-operation",
            uri: "/v1beta/models/aaaaaaaaaaaa:countTokens",
            body: r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
            expected_code: "unsupported_provider_operation",
        },
        ProviderErrorCase {
            label: "conformance-rerank-unsupported-capability",
            uri: "/v1/rerank",
            body: r#"{"provider":"openai","model":"text-rerank-001","query":"refund policy","documents":["refunds are processed in 3 days"],"top_n":1}"#,
            expected_code: "unsupported_provider_capability",
        },
        ProviderErrorCase {
            label: "conformance-embedding-unsupported-field",
            uri: "/v1/embeddings",
            body: r#"{"provider":"gemini","model":"gemini-embedding-001","input":"hello","dimensions":384}"#,
            expected_code: "unsupported_provider_field",
        },
        ProviderErrorCase {
            label: "conformance-image-unsupported-capability",
            uri: "/v1/images/generations",
            body: r#"{"provider":"anthropic","model":"claude-3-5-sonnet-latest","prompt":"draw a small house"}"#,
            expected_code: "unsupported_provider_capability",
        },
    ];

    for case in cases {
        let response = post_json(rest_state(case.label), case.uri, case.body).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{}", case.label);
        let body = json_body(response).await;
        assert_eq!(
            body["error"], case.expected_code,
            "{}: unexpected body {body}",
            case.label
        );
    }
}

#[tokio::test]
async fn native_routes_have_release_smoke_coverage() {
    let chat_response = post_json(
        rest_state("conformance-native-chat"),
        "/v1/chat",
        r#"{"model_ref":"aaaaaaaaaaaa","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
    )
    .await;
    assert_eq!(chat_response.status(), StatusCode::OK);
    assert_event_stream(&chat_response, "native-chat");
    let chat_body = sse_body(chat_response).await;
    assert!(chat_body.contains("event: error"));
    assert!(chat_body.contains("chat_model_failed"));

    let embeddings_response = post_json(
        rest_state("conformance-native-embeddings"),
        "/v1/embeddings",
        r#"{"model_ref":"aaaaaaaaaaaa","input":[]}"#,
    )
    .await;
    assert_eq!(embeddings_response.status(), StatusCode::BAD_REQUEST);
    let embeddings_body = json_body(embeddings_response).await;
    assert_eq!(embeddings_body["error"], "bad_request");

    let rerank_response = post_json(
        rest_state("conformance-native-rerank"),
        "/v1/rerank",
        r#"{"model_ref":"aaaaaaaaaaaa","query":"refund policy","documents":["refunds are processed in 3 days"],"top_n":1}"#,
    )
    .await;
    assert_eq!(rerank_response.status(), StatusCode::NOT_FOUND);
    let rerank_body = json_body(rerank_response).await;
    assert_eq!(rerank_body["error"], "not_found");
}

#[tokio::test]
async fn durable_media_job_and_result_routes_have_release_smoke_coverage() {
    assert_audio_transcription_upload_job_shape().await;
    assert_audio_speech_job_shape().await;
    assert_image_generation_job_shape().await;
    assert_video_understanding_upload_job_shape().await;
    assert_pending_result_route(
        "conformance-audio-transcription-pending",
        JobKind::audio_transcription(),
        "/v1/audio/transcriptions/job",
        "result",
    )
    .await;
    assert_pending_result_route(
        "conformance-audio-speech-pending",
        JobKind::audio_speech(),
        "/v1/audio/speech/job",
        "result",
    )
    .await;
    assert_pending_result_route(
        "conformance-video-understanding-pending",
        JobKind::video_understanding(),
        "/v1/video/understanding/job",
        "result",
    )
    .await;
    assert_pending_file_route(
        "conformance-image-generation-pending",
        JobKind::image_generation(),
        "/v1/images/generations/job",
        "files",
    )
    .await;
    assert_pending_file_route(
        "conformance-image-transform-pending",
        JobKind::image_generation(),
        "/v1/images/transforms/job",
        "files",
    )
    .await;
    assert_pending_file_route(
        "conformance-image-inpaint-pending",
        JobKind::image_generation(),
        "/v1/images/inpaint/job",
        "files",
    )
    .await;
    assert_pending_file_route(
        "conformance-image-control-pending",
        JobKind::image_generation(),
        "/v1/images/control/job",
        "files",
    )
    .await;
}

struct ProviderStreamCase {
    label: &'static str,
    uri: &'static str,
    body: &'static str,
    expected_content: &'static str,
    expected_done: Option<&'static str>,
    forbidden_content: Option<&'static str>,
}

struct ProviderErrorCase {
    label: &'static str,
    uri: &'static str,
    body: &'static str,
    expected_code: &'static str,
}

async fn assert_audio_transcription_upload_job_shape() {
    let requested_home = unique_home("conformance-audio-transcription-job");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "c".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["audio-transcription"]);
    let boundary = "tentgent-conformance-audio-upload";
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("output_format", "text"),
            MultipartPart::file("file", "input.wav", "audio/wav", b"not real audio"),
        ],
    );

    let response = post_multipart(
        state.clone(),
        "/v1/audio/transcriptions/job",
        boundary,
        body,
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    assert_eq!(body["job"]["kind"], "audio_transcription");
    assert_eq!(body["job"]["target"]["section"], "audio");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    assert_eq!(body["job"]["workspace"]["input"]["state"], "done");

    wait_for_terminal_or_yield(&state, body["job"]["job_id"].as_str().expect("job id")).await;
    let _ = fs::remove_dir_all(home);
}

async fn assert_audio_speech_job_shape() {
    let requested_home = unique_home("conformance-audio-speech-job");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "d".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["audio-speech"]);

    let response = post_json(
        state.clone(),
        "/v1/audio/speech/job",
        &format!(
            r#"{{"model_ref":"{model_ref}","text":"Hello from the conformance smoke.","output_format":"wav"}}"#
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    assert_eq!(body["job"]["kind"], "audio_speech");
    assert_eq!(body["job"]["target"]["section"], "audio");
    assert_eq!(body["job"]["target"]["reference"], model_ref);

    wait_for_terminal_or_yield(&state, body["job"]["job_id"].as_str().expect("job id")).await;
    let _ = fs::remove_dir_all(home);
}

async fn assert_image_generation_job_shape() {
    let requested_home = unique_home("conformance-image-generation-job");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "e".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["image-generation"]);

    let response = post_json(
        state.clone(),
        "/v1/images/generations/job",
        &format!(
            r#"{{"model_ref":"{model_ref}","prompt":"draw a small house","output_format":"png","width":512,"height":512}}"#
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    assert_eq!(body["job"]["kind"], "image_generation");
    assert_eq!(body["job"]["target"]["section"], "image");
    assert_eq!(body["job"]["target"]["reference"], model_ref);

    wait_for_terminal_or_yield(&state, body["job"]["job_id"].as_str().expect("job id")).await;
    let _ = fs::remove_dir_all(home);
}

async fn assert_video_understanding_upload_job_shape() {
    let requested_home = unique_home("conformance-video-understanding-job");
    let state = rest_state_for_home(requested_home);
    let home = state.app().layout().home_dir.canonicalize().expect("home");
    let model_ref = "f".repeat(64);
    write_safetensors_model_fixture_with_capabilities(&home, &model_ref, &["video-understanding"]);
    let boundary = "tentgent-conformance-video-upload";
    let body = multipart_body(
        boundary,
        &[
            MultipartPart::text("model_ref", &model_ref),
            MultipartPart::text("prompt", "summarize the clip"),
            MultipartPart::text("output_format", "text"),
            MultipartPart::file("file", "clip.mp4", "video/mp4", b"not real mp4"),
        ],
    );

    let response =
        post_multipart(state.clone(), "/v1/video/understanding/job", boundary, body).await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    assert_eq!(body["job"]["kind"], "video_understanding");
    assert_eq!(body["job"]["target"]["section"], "video");
    assert_eq!(body["job"]["target"]["reference"], model_ref);
    assert_eq!(body["job"]["workspace"]["input"]["state"], "done");

    wait_for_terminal_or_yield(&state, body["job"]["job_id"].as_str().expect("job id")).await;
    let _ = fs::remove_dir_all(home);
}

async fn assert_pending_result_route(
    label: &str,
    kind: JobKind,
    route_prefix: &str,
    result_suffix: &str,
) {
    let state = rest_state(label);
    let job =
        state
            .app()
            .jobs()
            .create(kind, "conformance pending job", None, Vec::<String>::new());
    let response = get(
        state,
        &format!("{route_prefix}/{}/{}", job.job_id, result_suffix),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT, "{label}");
    let body = json_body(response).await;
    assert_eq!(body["error"], "result_pending", "{label}");
}

async fn assert_pending_file_route(
    label: &str,
    kind: JobKind,
    route_prefix: &str,
    files_suffix: &str,
) {
    let state = rest_state(label);
    let job =
        state
            .app()
            .jobs()
            .create(kind, "conformance pending job", None, Vec::<String>::new());
    let response = get(
        state,
        &format!("{route_prefix}/{}/{}", job.job_id, files_suffix),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT, "{label}");
    let body = json_body(response).await;
    assert_eq!(body["error"], "result_pending", "{label}");
}

async fn post_json(state: RestState, uri: &str, body: &str) -> axum::response::Response {
    build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn post_multipart(
    state: RestState,
    uri: &str,
    boundary: &str,
    body: Vec<u8>,
) -> axum::response::Response {
    build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(
                    CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn get(state: RestState, uri: &str) -> axum::response::Response {
    build_router(state)
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn wait_for_terminal_or_yield(state: &RestState, job_id: &str) {
    let job_id = crate::runtime::JobId::new(job_id.to_string());
    for _ in 0..50 {
        let Some(job) = state.app().jobs().get(&job_id) else {
            return;
        };
        if job.status.is_terminal() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

fn assert_event_stream(response: &axum::response::Response, label: &str) {
    assert!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream")),
        "{label}"
    );
}
