use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use tentgent_kernel::foundation::error::KernelError;

use super::response::ErrorResponse;

#[derive(Debug)]
pub struct RestError {
    status: StatusCode,
    body: ErrorResponse,
}

impl RestError {
    pub fn bad_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn internal(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, code, message)
    }

    pub fn kernel(code: impl Into<String>, error: KernelError) -> Self {
        Self::internal(code, error.to_string())
    }

    fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ErrorResponse {
                error: code.into(),
                message: message.into(),
            },
        }
    }
}

impl IntoResponse for RestError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
