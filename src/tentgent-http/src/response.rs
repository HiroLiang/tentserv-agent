use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use tentgent_core::{
    server::ServerError, server_runtime::ServerRuntimeError, session::SessionError,
};

use crate::{
    dto::ErrorResponse,
    http::{HttpBody, HttpRequest, HttpResponse},
};

pub(crate) fn manager_error_response(context: &str, error: impl std::fmt::Display) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "store_read_failed",
            message: format!("failed to read {context}: {error}"),
        },
    )
}

pub(crate) fn server_error_response(error: ServerError) -> HttpResponse {
    match error {
        bad_request @ (ServerError::EmptyHost | ServerError::EmptyCloudProviderModel { .. }) => {
            json_response(
                400,
                ErrorResponse {
                    error: "bad_request",
                    message: bad_request.to_string(),
                },
            )
        }
        ServerError::NotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("server reference `{reference}` was not found"),
            },
        ),
        ServerError::AmbiguousRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "server reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        ServerError::AlreadyRunning(reference) => json_response(
            409,
            ErrorResponse {
                error: "already_running",
                message: format!("server `{reference}` is already running"),
            },
        ),
        ServerError::NotRunning(reference) => json_response(
            409,
            ErrorResponse {
                error: "not_running",
                message: format!("server `{reference}` is not running"),
            },
        ),
        other => json_response(
            500,
            ErrorResponse {
                error: "server_read_failed",
                message: format!("failed to read servers: {other}"),
            },
        ),
    }
}

pub(crate) fn session_error_response(error: SessionError) -> HttpResponse {
    match error {
        SessionError::NotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("session reference `{reference}` was not found"),
            },
        ),
        SessionError::AmbiguousRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "session reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        other => json_response(
            500,
            ErrorResponse {
                error: "session_read_failed",
                message: format!("failed to read sessions: {other}"),
            },
        ),
    }
}

pub(crate) fn runtime_error_response(error: ServerRuntimeError) -> HttpResponse {
    if error.is_provider_auth_error() {
        return json_response(
            409,
            ErrorResponse {
                error: "provider_auth_failed",
                message: error.to_string(),
            },
        );
    }
    if let Some(server_error) = error.as_server_error() {
        return server_error_response_for_ref(server_error);
    }

    json_response(
        500,
        ErrorResponse {
            error: "runtime_launch_failed",
            message: error.to_string(),
        },
    )
}

fn server_error_response_for_ref(error: &ServerError) -> HttpResponse {
    match error {
        ServerError::EmptyHost | ServerError::EmptyCloudProviderModel { .. } => json_response(
            400,
            ErrorResponse {
                error: "bad_request",
                message: error.to_string(),
            },
        ),
        ServerError::NotFound(reference) => json_response(
            404,
            ErrorResponse {
                error: "not_found",
                message: format!("server reference `{reference}` was not found"),
            },
        ),
        ServerError::AmbiguousRef(reference) => json_response(
            409,
            ErrorResponse {
                error: "ambiguous_ref",
                message: format!(
                    "server reference `{reference}` is ambiguous; use a longer prefix"
                ),
            },
        ),
        ServerError::AlreadyRunning(reference) => json_response(
            409,
            ErrorResponse {
                error: "already_running",
                message: format!("server `{reference}` is already running"),
            },
        ),
        ServerError::NotRunning(reference) => json_response(
            409,
            ErrorResponse {
                error: "not_running",
                message: format!("server `{reference}` is not running"),
            },
        ),
        other => json_response(
            500,
            ErrorResponse {
                error: "server_read_failed",
                message: format!("failed to read servers: {other}"),
            },
        ),
    }
}

pub(crate) fn parse_json_body<T: DeserializeOwned>(
    request: &HttpRequest,
) -> Result<T, HttpResponse> {
    if request.body.is_empty() {
        return Err(bad_request_response("request body must not be empty"));
    }
    serde_json::from_slice(&request.body)
        .map_err(|error| bad_request_response(format!("invalid JSON request body: {error}")))
}

pub(crate) fn bad_request_response(message: impl Into<String>) -> HttpResponse {
    json_response(
        400,
        ErrorResponse {
            error: "bad_request",
            message: message.into(),
        },
    )
}

pub(crate) fn method_not_allowed(request: &HttpRequest) -> HttpResponse {
    json_response(
        405,
        ErrorResponse {
            error: "method_not_allowed",
            message: format!("{} is not supported for {}", request.method, request.path),
        },
    )
}

pub(crate) fn not_found_response(path: &str) -> HttpResponse {
    json_response(
        404,
        ErrorResponse {
            error: "not_found",
            message: format!("route `{path}` was not found"),
        },
    )
}

pub(crate) fn unauthorized_response() -> HttpResponse {
    let mut response = json_response(
        401,
        ErrorResponse {
            error: "unauthorized",
            message: "missing or invalid daemon bearer token".to_string(),
        },
    );
    response
        .headers
        .push(("WWW-Authenticate".to_string(), "Bearer".to_string()));
    response
}

pub(crate) fn json_response(status_code: u16, body: impl Serialize) -> HttpResponse {
    let body = serde_json::to_vec(&body).unwrap_or_else(|_| {
        json!({
            "error": "response_encoding_failed",
            "message": "failed to encode JSON response"
        })
        .to_string()
        .into_bytes()
    });

    raw_response(
        status_code,
        "application/json; charset=utf-8",
        None,
        HttpBody::Buffered(body),
    )
}

pub(crate) fn raw_response(
    status_code: u16,
    content_type: impl Into<String>,
    cache_control: Option<String>,
    body: HttpBody,
) -> HttpResponse {
    HttpResponse {
        status_code,
        content_type: content_type.into(),
        cache_control,
        headers: Vec::new(),
        body,
    }
}
