use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use tower::ServiceExt;

use crate::transport::rest::build_router;

use super::{json_body, rest_state, sse_body};

const MESSAGES: &str = "/v1/messages";
const MODEL_REF: &str = "aaaaaaaaaaaa";

#[tokio::test]
async fn claude_messages_stream_uses_anthropic_sse_shape() {
    let response = post_messages(
        "claude-messages-stream-shape",
        r#"{"model":"aaaaaaaaaaaa","max_tokens":12,"messages":[{"role":"user","content":"hi"}],"stream":true}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_event_stream(&response);
    let body = sse_body(response).await;
    assert!(body.contains("event: message_start"));
    assert!(body.contains("event: content_block_start"));
    assert!(body.contains("event: error"));
    assert!(body.contains(r#""type":"chat_model_failed""#));
    assert!(!body.contains("data: [DONE]"));
}

#[tokio::test]
async fn claude_messages_accepts_current_text_only_shape() {
    let response = post_messages(
        "claude-messages-current-text-shape",
        r#"{"model":"aaaaaaaaaaaa","max_tokens":12,"system":[{"type":"text","text":"Answer briefly."}],"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]},{"role":"assistant","content":"hello"},{"role":"user","content":"again"}],"temperature":0.2,"stream":true}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_event_stream(&response);
    let body = sse_body(response).await;
    assert!(body.contains("event: message_start"));
    assert!(body.contains(r#""model":"aaaaaaaaaaaa""#));
}

#[tokio::test]
async fn claude_messages_rejects_missing_required_fields() {
    for (label, body) in [
        (
            "model",
            r#"{"max_tokens":12,"messages":[{"role":"user","content":"hi"}]}"#,
        ),
        (
            "max-tokens",
            r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"user","content":"hi"}]}"#,
        ),
        ("messages", r#"{"model":"aaaaaaaaaaaa","max_tokens":12}"#),
    ] {
        let response = post_messages(&format!("claude-messages-missing-{label}"), body).await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

#[tokio::test]
async fn claude_messages_rejects_invalid_message_role() {
    assert_messages_error(
        "claude-messages-invalid-role",
        r#"{"model":"aaaaaaaaaaaa","max_tokens":12,"messages":[{"role":"tool","content":"hi"}],"stream":true}"#,
        "bad_request",
    )
    .await;
}

#[tokio::test]
async fn claude_messages_rejects_tool_fields() {
    for (label, field) in [
        (
            "tools",
            r#""tools":[{"name":"lookup","input_schema":{"type":"object"}}]"#,
        ),
        ("tool-choice", r#""tool_choice":{"type":"auto"}"#),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","max_tokens":12,"messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_messages_error(
            &format!("claude-messages-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn claude_messages_rejects_audio_fields() {
    for (label, field) in [
        ("audio", r#""audio":{"voice":"alloy","format":"wav"}"#),
        ("modalities", r#""modalities":["text","audio"]"#),
        (
            "input-audio",
            r#""input_audio":{"data":"AA==","format":"wav"}"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","max_tokens":12,"messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_messages_error(
            &format!("claude-messages-audio-field-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn claude_messages_rejects_message_audio_fields() {
    for (label, field) in [
        ("audio", r#""audio":{"id":"audio_1","data":"AA=="}"#),
        (
            "input-audio",
            r#""input_audio":{"data":"AA==","format":"wav"}"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","max_tokens":12,"messages":[{{"role":"user","content":"hi",{field}}}]}}"#
        );
        assert_messages_error(
            &format!("claude-messages-message-audio-field-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn claude_messages_rejects_unsupported_content_blocks() {
    for (label, content) in [
        (
            "audio",
            r#"[{"type":"audio","source":{"type":"base64","media_type":"audio/wav","data":"AA=="}}]"#,
        ),
        (
            "input-audio",
            r#"[{"type":"input_audio","input_audio":{"data":"AA==","format":"wav"}}]"#,
        ),
        (
            "image",
            r#"[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AA=="}}]"#,
        ),
        (
            "tool-use",
            r#"[{"type":"tool_use","id":"toolu_1","name":"lookup","input":{}}]"#,
        ),
        (
            "tool-result",
            r#"[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","max_tokens":12,"messages":[{{"role":"user","content":{content}}}],"stream":true}}"#
        );
        assert_messages_error(
            &format!("claude-messages-{label}"),
            &body,
            "unsupported_provider_content",
        )
        .await;
    }
}

#[tokio::test]
async fn claude_audio_transcription_and_speech_routes_are_not_provider_compatible() {
    for (label, route) in [
        ("transcription", "/v1/audio/transcriptions"),
        ("speech", "/v1/audio/speech"),
    ] {
        let response = post_json(
            &format!("claude-audio-{label}-route"),
            route,
            r#"{"model":"claude-audio","input":"AA=="}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

async fn assert_messages_error(label: &str, body: &str, expected_code: &str) {
    let response = post_messages(label, body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], expected_code);
}

async fn post_messages(label: &str, body: &str) -> axum::response::Response {
    post_json(label, MESSAGES, body).await
}

async fn post_json(label: &str, route: &str, body: &str) -> axum::response::Response {
    let state = rest_state(label);
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(route)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}

fn assert_event_stream(response: &axum::response::Response) {
    assert!(response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream")));
}
