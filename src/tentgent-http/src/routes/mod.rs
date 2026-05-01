pub(crate) mod chat;
pub(crate) mod lifecycle;
pub(crate) mod status;
pub(crate) mod store;

use crate::{
    app::DaemonHttpState,
    dto::ErrorResponse,
    http::{HttpRequest, HttpResponse},
    response::{json_response, method_not_allowed, not_found_response},
};

pub(crate) async fn route_request(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    if let Some(error) = &request.parse_error {
        return json_response(
            error.status_code,
            ErrorResponse {
                error: "bad_request",
                message: error.message.clone(),
            },
        );
    }

    if request.version != "HTTP/1.1" && request.version != "HTTP/1.0" {
        return json_response(
            400,
            ErrorResponse {
                error: "bad_request",
                message: "unsupported HTTP version".to_string(),
            },
        );
    }

    match request.method.as_str() {
        "GET" => route_get(request, state).await,
        "POST" => route_post(request, state).await,
        _ => method_not_allowed(request),
    }
}

async fn route_get(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    match request.path.as_str() {
        "/healthz" => status::healthz_response(),
        "/v1/status" => status::status_response(state),
        "/v1/models" => store::list_models_response(state),
        "/v1/adapters" => store::list_adapters_response(state),
        "/v1/datasets" => store::list_datasets_response(state),
        "/v1/servers" => store::list_servers_response(state),
        path if server_action_path(path).is_some() => method_not_allowed(request),
        path if server_health_path(path).is_some() => match server_health_path(path) {
            Some(reference) => lifecycle::health_server_response(state, reference).await,
            None => not_found_response(&request.path),
        },
        path if path.starts_with("/v1/servers/") => {
            let reference = path.trim_start_matches("/v1/servers/");
            if reference.is_empty() {
                not_found_response(&request.path)
            } else {
                store::inspect_server_response(state, reference)
            }
        }
        _ => not_found_response(&request.path),
    }
}

async fn route_post(request: &HttpRequest, state: &DaemonHttpState) -> HttpResponse {
    match request.path.as_str() {
        "/v1/chat" => chat::proxy_chat_response(state, request).await,
        "/v1/servers" => lifecycle::create_server_response(state, request),
        path if path.starts_with("/v1/servers/") => match server_action_path(path) {
            Some((reference, ServerAction::Start)) => {
                lifecycle::start_server_response(state, reference, request).await
            }
            Some((reference, ServerAction::Stop)) => {
                lifecycle::stop_server_response(state, reference)
            }
            None => not_found_response(&request.path),
        },
        _ => not_found_response(&request.path),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerAction {
    Start,
    Stop,
}

fn server_action_path(path: &str) -> Option<(&str, ServerAction)> {
    let rest = path.strip_prefix("/v1/servers/")?;
    let (reference, action) = rest.rsplit_once('/')?;
    if reference.is_empty() {
        return None;
    }

    match action {
        "start" => Some((reference, ServerAction::Start)),
        "stop" => Some((reference, ServerAction::Stop)),
        _ => None,
    }
}

fn server_health_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/servers/")?;
    let reference = rest.strip_suffix("/health")?;
    if reference.is_empty() || reference.contains('/') {
        return None;
    }
    Some(reference)
}

#[cfg(test)]
mod tests;
