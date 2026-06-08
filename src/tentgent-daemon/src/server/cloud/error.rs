use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tentgent_kernel::foundation::error::KernelError;

use crate::provider_compat::ProviderCompatRejection;

#[derive(Debug)]
pub(super) struct CloudServerError {
    pub(super) status: axum::http::StatusCode,
    pub(super) code: &'static str,
    pub(super) message: String,
}

impl CloudServerError {
    pub(super) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }
}

impl From<ProviderCompatRejection> for CloudServerError {
    fn from(rejection: ProviderCompatRejection) -> Self {
        let (code, message) = rejection.into_parts();
        Self {
            status: axum::http::StatusCode::BAD_REQUEST,
            code,
            message,
        }
    }
}

impl From<KernelError> for CloudServerError {
    fn from(error: KernelError) -> Self {
        match error {
            KernelError::UnsupportedTarget(message) => {
                ProviderCompatRejection::unsupported_capability(message).into()
            }
            other => Self {
                status: axum::http::StatusCode::BAD_GATEWAY,
                code: "cloud_runtime_failed",
                message: other.to_string(),
            },
        }
    }
}

impl IntoResponse for CloudServerError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.code,
                "message": self.message,
            })),
        )
            .into_response()
    }
}
