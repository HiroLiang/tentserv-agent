use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use tower::ServiceExt;

use crate::transport::rest::build_router;

use super::{json_body, rest_state};

const RERANK: &str = "/v1/rerank";
const MODEL_REF: &str = "aaaaaaaaaaaa";

#[tokio::test]
async fn provider_rerank_rejects_openai_shaped_body() {
    assert_provider_rerank_unsupported(
        "provider-rerank-openai",
        r#"{"provider":"openai","model":"text-rerank-001","query":"refund policy","documents":["refunds are processed in 3 days"],"top_n":1}"#,
    )
    .await;
}

#[tokio::test]
async fn provider_rerank_rejects_claude_shaped_body() {
    assert_provider_rerank_unsupported(
        "provider-rerank-claude",
        r#"{"provider":"anthropic","model":"claude-3-5-sonnet-latest","query":"refund policy","documents":["refunds are processed in 3 days"],"top_n":1}"#,
    )
    .await;
}

#[tokio::test]
async fn provider_rerank_rejects_gemini_shaped_body() {
    assert_provider_rerank_unsupported(
        "provider-rerank-gemini",
        r#"{"provider":"gemini","model":"semantic-ranker-default@latest","query":"refund policy","documents":["refunds are processed in 3 days"],"top_n":1}"#,
    )
    .await;
}

#[tokio::test]
async fn provider_rerank_rejects_model_selector_without_provider() {
    assert_provider_rerank_unsupported(
        "provider-rerank-model-selector",
        r#"{"model":"text-rerank-001","query":"refund policy","documents":["refunds are processed in 3 days"],"top_n":1}"#,
    )
    .await;
}

#[tokio::test]
async fn native_rerank_unknown_fields_remain_native_bad_request() {
    let response = post_rerank(
        "native-rerank-unknown-field",
        &format!(
            r#"{{"model_ref":"{MODEL_REF}","query":"refund policy","documents":["refunds are processed in 3 days"],"session_ref":"session"}}"#
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn native_rerank_body_still_uses_model_ref() {
    let response = post_rerank(
        "native-rerank-model-ref",
        &format!(
            r#"{{"model_ref":"{MODEL_REF}","query":"refund policy","documents":["refunds are processed in 3 days"],"top_n":1}}"#
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"], "not_found");
}

async fn assert_provider_rerank_unsupported(label: &str, body: &str) {
    let response = post_rerank(label, body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], "unsupported_provider_capability");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("provider-compatible rerank is not supported"));
    assert!(message.contains("model_ref"));
}

async fn post_rerank(label: &str, body: &str) -> axum::response::Response {
    let state = rest_state(label);
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(RERANK)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}
