use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use tower::ServiceExt;

use crate::transport::rest::build_router;

use super::{json_body, rest_state};

const EMBEDDINGS: &str = "/v1/embeddings";
const MODEL: &str = "text-embedding-3-small";

#[tokio::test]
async fn openai_embeddings_rejects_empty_input() {
    assert_embedding_error(
        "openai-embeddings-empty-input",
        &format!(r#"{{"model":"{MODEL}","input":[]}}"#),
        "bad_request",
    )
    .await;
}

#[tokio::test]
async fn openai_embeddings_rejects_empty_string_input() {
    assert_embedding_error(
        "openai-embeddings-empty-string",
        &format!(r#"{{"model":"{MODEL}","input":" "}}"#),
        "bad_request",
    )
    .await;
}

#[tokio::test]
async fn openai_embeddings_rejects_invalid_input_type() {
    for (label, input) in [
        ("object", r#"{"text":"hello"}"#),
        ("number-array", "[1,2,3]"),
    ] {
        let body = format!(r#"{{"model":"{MODEL}","input":{input}}}"#);
        assert_embedding_error(
            &format!("openai-embeddings-invalid-input-{label}"),
            &body,
            "bad_request",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_embeddings_rejects_unsupported_fields() {
    for (label, field) in [
        ("dimensions", r#""dimensions":384"#),
        ("base64", r#""encoding_format":"base64""#),
        ("user", r#""user":"legacy-user-id""#),
        ("unknown", r#""unknown_field":"value""#),
    ] {
        let body = format!(r#"{{"model":"{MODEL}","input":"hello",{field}}}"#);
        assert_embedding_error(
            &format!("openai-embeddings-unsupported-{label}"),
            &body,
            "unsupported_provider_field",
        )
        .await;
    }
}

#[tokio::test]
async fn openai_embeddings_rejects_anthropic_provider_capability() {
    assert_embedding_error(
        "openai-embeddings-anthropic-provider",
        r#"{"model":"claude-3-5-sonnet-latest","provider":"anthropic","input":"hello"}"#,
        "unsupported_provider_capability",
    )
    .await;
}

async fn assert_embedding_error(label: &str, body: &str, code: &str) {
    let response = post_embeddings(label, body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"], code);
}

async fn post_embeddings(label: &str, body: &str) -> axum::response::Response {
    let state = rest_state(label);
    build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(EMBEDDINGS)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}
