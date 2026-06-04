use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use tower::ServiceExt;

use crate::transport::rest::build_router;

use super::{json_body, rest_state, sse_body};

const CHAT_COMPLETIONS: &str = "/v1/chat/completions";
const MODEL_REF: &str = "aaaaaaaaaaaa";

#[tokio::test]
async fn openai_chat_completions_stream_uses_openai_sse_shape() {
    let response = post_chat_completions(
        "openai-chat-stream",
        r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_event_stream(&response);
    let body = sse_body(response).await;
    assert!(body.contains(r#""object":"chat.completion.chunk""#));
    assert!(body.contains(r#""model":"aaaaaaaaaaaa""#));
    assert!(body.contains(r#""type":"chat_model_failed""#));
    assert!(body.contains("data: [DONE]"));
    assert!(!body.contains("event: error"));
}

#[tokio::test]
async fn openai_chat_completions_accepts_current_text_only_chat_shape() {
    let response = post_chat_completions(
        "openai-chat-current-text-shape",
        r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"developer","content":[{"type":"text","text":"Follow policy."}],"name":"policy"},{"role":"system","content":"Keep answers short."},{"role":"user","content":[{"type":"text","text":"hi"}],"name":"tester"},{"role":"assistant","content":[{"type":"text","text":"hello"}]}],"max_completion_tokens":12,"temperature":0.2,"stream":true,"stream_options":{"include_usage":false,"include_obfuscation":false},"modalities":["text"],"response_format":{"type":"text"},"tool_choice":"none","function_call":"none","parallel_tool_calls":false,"n":1,"store":false}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_event_stream(&response);
    let body = sse_body(response).await;
    assert!(body.contains(r#""object":"chat.completion.chunk""#));
    assert!(body.contains(r#""type":"chat_model_failed""#));
    assert!(body.contains("data: [DONE]"));
}

#[tokio::test]
async fn openai_chat_completions_accepts_unknown_top_level_fields_for_now() {
    let response = post_chat_completions(
        "openai-chat-unknown-field",
        r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"user","content":"hi"}],"unknown_field":"ignored-for-now","stream":true}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_event_stream(&response);
}

#[tokio::test]
async fn openai_chat_completions_rejects_invalid_message_role() {
    assert_chat_error(
        "openai-chat-invalid-role",
        r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"tool","content":"hi"}]}"#,
        "unsupported_provider_content",
    )
    .await;
}

#[tokio::test]
async fn openai_chat_completions_rejects_tool_fields() {
    for (label, field) in [
        (
            "tools",
            r#""tools":[{"type":"function","function":{"name":"lookup"}}]"#,
        ),
        ("tool-choice", r#""tool_choice":"auto""#),
        ("parallel-tool-calls", r#""parallel_tool_calls":true"#),
        (
            "functions",
            r#""functions":[{"name":"lookup","parameters":{"type":"object"}}]"#,
        ),
        ("function-call", r#""function_call":"auto""#),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_chat_error(
            &format!("openai-chat-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_chat_completions_rejects_response_format() {
    for (label, field) in [
        ("json-object", r#""response_format":{"type":"json_object"}"#),
        (
            "json-schema",
            r#""response_format":{"type":"json_schema","json_schema":{"name":"answer","schema":{"type":"object"}}}"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_chat_error(
            &format!("openai-chat-response-format-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_chat_completions_rejects_audio_and_non_text_modalities() {
    for (label, field) in [
        ("audio", r#""audio":{"voice":"alloy","format":"wav"}"#),
        ("modalities", r#""modalities":["text","audio"]"#),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_chat_error(
            &format!("openai-chat-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_chat_completions_rejects_message_tool_calls() {
    assert_chat_error(
        "openai-chat-message-tool-calls",
        r#"{"model":"aaaaaaaaaaaa","messages":[{"role":"assistant","content":"hi","tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{}"}}]}]}"#,
        "unsupported_provider_field",
    )
    .await;
}

#[tokio::test]
async fn openai_chat_completions_rejects_non_text_content_parts_on_daemon_route() {
    for (label, content) in [
        (
            "image-url",
            r#"[{"type":"image_url","image_url":{"url":"data:image/png;base64,AA=="}}]"#,
        ),
        (
            "input-audio",
            r#"[{"type":"input_audio","input_audio":{"data":"AA==","format":"wav"}}]"#,
        ),
        ("file", r#"[{"type":"file","file":{"file_id":"file_123"}}]"#),
        (
            "refusal",
            r#"[{"type":"refusal","refusal":"cannot answer"}]"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":{content}}}]}}"#
        );
        assert_chat_error(
            &format!("openai-chat-{label}"),
            &body,
            "unsupported_provider_content",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_chat_completions_rejects_current_unsupported_generation_fields() {
    for (label, field) in [
        ("n-multiple", r#""n":2"#),
        ("stop", r#""stop":"END""#),
        ("top-p", r#""top_p":0.8"#),
        ("frequency-penalty", r#""frequency_penalty":0.2"#),
        ("presence-penalty", r#""presence_penalty":0.2"#),
        ("logit-bias", r#""logit_bias":{"42":1}"#),
        ("logprobs", r#""logprobs":true"#),
        ("top-logprobs", r#""top_logprobs":2"#),
        (
            "prediction",
            r#""prediction":{"type":"content","content":"known output"}"#,
        ),
        ("reasoning-effort", r#""reasoning_effort":"low""#),
        ("verbosity", r#""verbosity":"low""#),
        (
            "stream-usage",
            r#""stream":true,"stream_options":{"include_usage":true}"#,
        ),
        (
            "web-search",
            r#""web_search_options":{"search_context_size":"low"}"#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_chat_error(
            &format!("openai-chat-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_chat_completions_rejects_current_unsupported_provider_metadata_fields() {
    for (label, field) in [
        ("metadata", r#""metadata":{"topic":"demo"}"#),
        ("store", r#""store":true"#),
        ("seed", r#""seed":42"#),
        ("service-tier", r#""service_tier":"flex""#),
        ("user", r#""user":"legacy-user-id""#),
        ("safety-identifier", r#""safety_identifier":"safe-user-id""#),
        ("prompt-cache-key", r#""prompt_cache_key":"cache-key""#),
        (
            "prompt-cache-retention",
            r#""prompt_cache_retention":"24h""#,
        ),
    ] {
        let body = format!(
            r#"{{"model":"{MODEL_REF}","messages":[{{"role":"user","content":"hi"}}],{field}}}"#
        );
        assert_chat_error(
            &format!("openai-chat-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

async fn assert_chat_error(label: &str, body: &str, expected_code: &str) {
    let response = post_chat_completions(label, body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], expected_code);
}

async fn post_chat_completions(label: &str, body: &str) -> axum::response::Response {
    let state = rest_state(label);
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CHAT_COMPLETIONS)
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
