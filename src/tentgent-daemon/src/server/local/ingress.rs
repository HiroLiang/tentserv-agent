use axum::http::{Method, StatusCode};

use super::error::LocalServerError;

// Only these native media/rerank endpoints may use transparent forwarding.
// Chat/provider endpoints must pass through their managed Rust handlers.
pub(super) const PUBLIC_RUNTIME_PATHS: &[&str] = &[
    "/v1/audio/transcriptions",
    "/v1/audio/speech",
    "/v1/images/transforms",
    "/v1/images/inpaint",
    "/v1/images/control",
    "/v1/rerank",
    "/v1/video/understanding",
    "/v1/vision/chat",
];

pub(super) fn ensure_public_runtime_request(
    method: &Method,
    path: &str,
) -> Result<(), LocalServerError> {
    // Match the original path exactly. Decoding or redirecting aliases could
    // turn a fallback into a bypass of a managed endpoint's DTO validation.
    if !PUBLIC_RUNTIME_PATHS.contains(&path) {
        return Err(unsupported_route());
    }
    if method != Method::POST {
        return Err(LocalServerError {
            status: StatusCode::METHOD_NOT_ALLOWED,
            code: "method_not_allowed",
            message: "native inference endpoints require POST".to_string(),
        });
    }
    Ok(())
}

pub(super) async fn reject_unknown_route() -> LocalServerError {
    unsupported_route()
}

fn unsupported_route() -> LocalServerError {
    LocalServerError {
        status: StatusCode::NOT_FOUND,
        code: "not_found",
        message: "route is not a public model server endpoint".to_string(),
    }
}
