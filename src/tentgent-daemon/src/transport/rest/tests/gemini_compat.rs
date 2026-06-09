use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use tower::ServiceExt;

use crate::transport::rest::build_router;

use super::{json_body, rest_state, sse_body};

const MODEL_REF: &str = "aaaaaaaaaaaa";

#[tokio::test]
async fn gemini_stream_generate_content_uses_gemini_sse_shape() {
    let response = post_gemini(
        "gemini-stream-current-shape",
        &format!("/v1beta/models/{MODEL_REF}:streamGenerateContent?alt=sse"),
        r#"{"systemInstruction":{"parts":[{"text":"Answer briefly."}]},"contents":[{"role":"user","parts":[{"text":"hi"}]}],"generationConfig":{"maxOutputTokens":12,"temperature":0.2}}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_event_stream(&response);
    let body = sse_body(response).await;
    assert!(body.contains(r#""error":{"code":"chat_model_failed""#));
    assert!(!body.contains("event:"));
    assert!(!body.contains("data: [DONE]"));
}

#[tokio::test]
async fn gemini_generate_content_rejects_missing_contents() {
    let response = post_gemini(
        "gemini-generate-missing-contents",
        &format!("/v1beta/models/{MODEL_REF}:generateContent"),
        r#"{"generationConfig":{"maxOutputTokens":12}}"#,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn gemini_generate_content_rejects_non_text_parts_on_daemon_route() {
    for (label, part) in [
        (
            "inline-data-camel-case",
            r#"{"inlineData":{"mimeType":"image/png","data":"AA=="}}"#,
        ),
        (
            "inline-data-snake-case",
            r#"{"inline_data":{"mime_type":"image/jpeg","data":"AA=="}}"#,
        ),
        (
            "file-data-camel-case",
            r#"{"fileData":{"mimeType":"image/png","fileUri":"gs://bucket/image.png"}}"#,
        ),
        (
            "file-data-snake-case",
            r#"{"file_data":{"mime_type":"image/png","file_uri":"gs://bucket/image.png"}}"#,
        ),
        (
            "unknown-part",
            r#"{"functionCall":{"name":"lookup","args":{}}}"#,
        ),
    ] {
        let body = format!(
            r#"{{"contents":[{{"role":"user","parts":[{{"text":"Describe this."}},{part}]}}]}}"#
        );
        assert_gemini_error(
            &format!("gemini-generate-{label}"),
            &format!("/v1beta/models/{MODEL_REF}:generateContent"),
            &body,
            "unsupported_provider_content",
        )
        .await;
    }
}

#[tokio::test]
async fn gemini_generate_content_rejects_tool_fields() {
    for (label, field) in [
        (
            "tools",
            r#""tools":[{"functionDeclarations":[{"name":"lookup"}]}]"#,
        ),
        (
            "tool-config",
            r#""toolConfig":{"functionCallingConfig":{"mode":"AUTO"}}"#,
        ),
    ] {
        let body =
            format!(r#"{{"contents":[{{"role":"user","parts":[{{"text":"hi"}}]}}],{field}}}"#);
        assert_gemini_error(
            &format!("gemini-generate-{label}"),
            &format!("/v1beta/models/{MODEL_REF}:generateContent"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn gemini_generate_content_rejects_unsupported_operation() {
    assert_gemini_error(
        "gemini-generate-count-tokens",
        &format!("/v1beta/models/{MODEL_REF}:countTokens"),
        r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
        "unsupported_provider_operation",
    )
    .await;
}

#[tokio::test]
async fn gemini_embeddings_reject_unsupported_openai_fields_before_dispatch() {
    assert_embedding_error(
        "gemini-embeddings-dimensions",
        r#"{"provider":"gemini","model":"gemini-embedding-001","input":"hello","dimensions":384}"#,
        "unsupported_provider_field",
    )
    .await;
}

async fn assert_gemini_error(label: &str, uri: &str, body: &str, code: &str) {
    let response = post_gemini(label, uri, body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], code);
}

async fn assert_embedding_error(label: &str, body: &str, code: &str) {
    let response = post_gemini(label, "/v1/embeddings", body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], code);
}

async fn post_gemini(label: &str, uri: &str, body: &str) -> axum::response::Response {
    let state = rest_state(label);
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(CONTENT_TYPE, "application/json")
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
