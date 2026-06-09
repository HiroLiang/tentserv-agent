use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use tower::ServiceExt;

use crate::transport::rest::build_router;

use super::{json_body, rest_state};

const CHAT_COMPLETIONS: &str = "/v1/chat/completions";
const MODEL_REF: &str = "aaaaaaaaaaaa";

#[tokio::test]
async fn openai_chat_completions_rejects_audio_input_parts_on_daemon_route() {
    for (label, content) in [
        (
            "wav",
            r#"[{"type":"input_audio","input_audio":{"data":"AA==","format":"wav"}}]"#,
        ),
        (
            "mp3",
            r#"[{"type":"input_audio","input_audio":{"data":"AA==","format":"mp3"}}]"#,
        ),
        (
            "mixed-text-and-audio",
            r#"[{"type":"text","text":"Transcribe this."},{"type":"input_audio","input_audio":{"data":"AA==","format":"wav"}}]"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":{content}}}]}}"#
        );
        assert_chat_error(
            &format!("openai-chat-audio-input-{label}"),
            &body,
            "unsupported_provider_content",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_chat_completions_rejects_audio_output_options_on_daemon_route() {
    for (label, field) in [
        ("audio", r#""audio":{"voice":"alloy","format":"wav"}"#),
        ("modalities", r#""modalities":["text","audio"]"#),
        (
            "audio-only-modalities",
            r#""modalities":["audio"],"audio":{"voice":"alloy","format":"mp3"}"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_chat_error(
            &format!("openai-chat-audio-output-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_audio_transcription_and_speech_routes_are_not_provider_compatible() {
    for (label, route) in [
        ("transcription", "/v1/audio/transcriptions"),
        ("speech", "/v1/audio/speech"),
    ] {
        let response = post_json(
            &format!("openai-audio-{label}-route"),
            route,
            r#"{"model":"gpt-audio","input":"AA=="}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

async fn assert_chat_error(label: &str, body: &str, expected_code: &str) {
    let response = post_json(label, CHAT_COMPLETIONS, body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], expected_code);
}

async fn post_json(label: &str, route: &str, body: &str) -> axum::response::Response {
    let state = rest_state(label);
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(route)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}
