use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use tentgent_kernel::{
    features::daemon::usecases::{DaemonInspectionMode, DaemonStatusRequest, DaemonStatusUseCase},
    foundation::layout::LayoutResolveMode,
};

use crate::transport::rest::{
    response::ErrorResponse,
    security::{DaemonSecurityConfig, DaemonTokenAuthorizationError, DAEMON_TOKEN_ENV_VAR},
    state::RestState,
};

use super::dto::{DaemonShutdownItem, DaemonShutdownResponse};

pub async fn shutdown(State(state): State<RestState>, headers: HeaderMap, body: Bytes) -> Response {
    let security = DaemonSecurityConfig::from_env();
    if !security.token_enabled() {
        return error_response(
            StatusCode::CONFLICT,
            "daemon_token_required",
            format!("daemon shutdown requires {DAEMON_TOKEN_ENV_VAR} to be enabled"),
        );
    }
    if let Err(err) = security.authorize_headers(&headers) {
        return match err {
            DaemonTokenAuthorizationError::Disabled => error_response(
                StatusCode::CONFLICT,
                "daemon_token_required",
                format!("daemon shutdown requires {DAEMON_TOKEN_ENV_VAR} to be enabled"),
            ),
            DaemonTokenAuthorizationError::Missing
            | DaemonTokenAuthorizationError::Malformed
            | DaemonTokenAuthorizationError::Mismatch => unauthorized_response(),
        };
    }
    if let Err(response) = validate_shutdown_body(&body) {
        return response;
    }

    let pid = state
        .app()
        .services()
        .daemon()
        .usecase()
        .daemon_status(DaemonStatusRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            mode: DaemonInspectionMode::Observational,
        })
        .ok()
        .and_then(|status| status.inspection.process.map(|process| process.pid));

    state.app().request_shutdown();

    (
        StatusCode::ACCEPTED,
        Json(DaemonShutdownResponse {
            shutdown: DaemonShutdownItem {
                accepted: true,
                pid,
                message: "daemon shutdown requested",
            },
        }),
    )
        .into_response()
}

fn validate_shutdown_body(body: &[u8]) -> Result<(), Response> {
    if body.is_empty() {
        return Ok(());
    }
    let value = serde_json::from_slice::<Value>(body).map_err(|err| {
        error_response(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid JSON request body: {err}"),
        )
    })?;
    match value {
        Value::Object(map) if map.is_empty() => Ok(()),
        Value::Object(_)
        | Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Array(_) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "shutdown request body must be empty or `{}` without fields",
        )),
    }
}

fn unauthorized_response() -> Response {
    let mut response = error_response(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "missing or invalid daemon bearer token",
    );
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn error_response(
    status: StatusCode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: code.into(),
            message: message.into(),
        }),
    )
        .into_response()
}
