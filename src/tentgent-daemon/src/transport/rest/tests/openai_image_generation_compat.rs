use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use tower::ServiceExt;

use crate::transport::rest::build_router;

use super::{json_body, rest_state};

const IMAGES: &str = "/v1/images/generations";
const MODEL: &str = "gpt-image-1";

#[tokio::test]
async fn openai_image_generation_rejects_missing_model() {
    assert_image_deserialization_error(
        "openai-image-generation-missing-model",
        r#"{"prompt":"A small red cube"}"#,
    )
    .await;
}

#[tokio::test]
async fn openai_image_generation_rejects_missing_prompt() {
    assert_image_deserialization_error(
        "openai-image-generation-missing-prompt",
        &format!(r#"{{"model":"{MODEL}"}}"#),
    )
    .await;
}

#[tokio::test]
async fn openai_image_generation_rejects_response_format() {
    assert_image_error(
        "openai-image-generation-response-format",
        &format!(
            r#"{{"model":"{MODEL}","prompt":"A small red cube","response_format":"b64_json"}}"#
        ),
        "unsupported_provider_field",
    )
    .await;
}

#[tokio::test]
async fn openai_image_generation_rejects_n() {
    assert_image_error(
        "openai-image-generation-n",
        &format!(r#"{{"model":"{MODEL}","prompt":"A small red cube","n":2}}"#),
        "unsupported_provider_field",
    )
    .await;
}

#[tokio::test]
async fn openai_image_generation_rejects_anthropic_provider_capability() {
    assert_image_error(
        "openai-image-generation-anthropic-provider",
        r#"{"provider":"anthropic","model":"claude-3-5-sonnet-latest","prompt":"A small red cube"}"#,
        "unsupported_provider_capability",
    )
    .await;
}

#[tokio::test]
async fn openai_image_generation_rejects_invalid_provider_string() {
    assert_image_deserialization_error(
        "openai-image-generation-invalid-provider",
        &format!(r#"{{"provider":"unknown","model":"{MODEL}","prompt":"A small red cube"}}"#),
    )
    .await;
}

async fn assert_image_deserialization_error(label: &str, body: &str) {
    let response = post_images(label, body).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

async fn assert_image_error(label: &str, body: &str, code: &str) {
    let response = post_images(label, body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], code);
}

async fn post_images(label: &str, body: &str) -> axum::response::Response {
    let state = rest_state(label);
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(IMAGES)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}
