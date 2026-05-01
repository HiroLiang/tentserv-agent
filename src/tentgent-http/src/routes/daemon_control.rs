use serde_json::Value;

use crate::{
    app::DaemonHttpState,
    dto::{DaemonShutdownItem, DaemonShutdownResponse, ErrorResponse},
    http::{HttpAfterWriteAction, HttpRequest, HttpResponse},
    response::{bad_request_response, json_response},
};

pub(crate) fn shutdown_response(state: &DaemonHttpState, request: &HttpRequest) -> HttpResponse {
    if !state.security().token_enabled() {
        return json_response(
            409,
            ErrorResponse {
                error: "daemon_token_required",
                message: "daemon shutdown requires TENTGENT_DAEMON_TOKEN to be enabled".to_string(),
            },
        );
    }
    if !request.body.is_empty() {
        let value = match serde_json::from_slice::<Value>(&request.body) {
            Ok(value) => value,
            Err(error) => {
                return bad_request_response(format!("invalid JSON request body: {error}"));
            }
        };
        match value {
            Value::Object(map) if map.is_empty() => {}
            Value::Object(_) => {
                return bad_request_response(
                    "shutdown request body must be empty or `{}` without fields",
                );
            }
            _ => {
                return bad_request_response(
                    "shutdown request body must be empty or `{}` without fields",
                );
            }
        }
    }

    let mut response = json_response(
        202,
        DaemonShutdownResponse {
            shutdown: DaemonShutdownItem {
                accepted: true,
                pid: state
                    .inspection()
                    .process
                    .as_ref()
                    .map(|process| process.pid),
                message: "daemon shutdown requested",
            },
        },
    );
    response.after_write = Some(HttpAfterWriteAction::RequestDaemonShutdown);
    response
}
