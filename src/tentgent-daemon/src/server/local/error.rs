use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::provider_compat::ProviderCompatRejection;

#[derive(Debug)]
pub(super) struct LocalServerError {
    pub(super) status: StatusCode,
    pub(super) code: &'static str,
    pub(super) message: String,
}

impl LocalServerError {
    pub(super) fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    pub(super) fn internal(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "local_proxy_failed",
            message,
        }
    }

    pub(super) fn bad_gateway(message: String) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "model_runtime_proxy_failed",
            message,
        }
    }
}

impl From<ProviderCompatRejection> for LocalServerError {
    fn from(rejection: ProviderCompatRejection) -> Self {
        let (code, message) = rejection.into_parts();
        Self::bad_request(code, message)
    }
}

impl IntoResponse for LocalServerError {
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
