use reqwest::header::{CACHE_CONTROL, CONTENT_TYPE};
use serde_json::Value;
use tentgent_core::server::{ServerInspection, ServerManager};

use crate::{
    app::DaemonHttpState,
    dto::ErrorResponse,
    http::{HttpBody, HttpRequest, HttpResponse},
    response::{
        bad_request_response, json_response, parse_json_body, raw_response, server_error_response,
    },
};

pub(crate) async fn proxy_chat_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let mut body = match parse_json_body::<Value>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let Some(body_object) = body.as_object_mut() else {
        return bad_request_response("request body must be a JSON object");
    };
    let server_reference = match chat_server_reference(body_object.get("server_ref")) {
        Ok(reference) => reference,
        Err(response) => return response,
    };
    body_object.remove("server_ref");

    let manager = match ServerManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return server_error_response(error),
    };
    let server = match select_chat_server(&manager, server_reference.as_deref()) {
        Ok(server) => server,
        Err(response) => return response,
    };
    let target = format!("http://{}:{}/v1/chat", server.spec.host, server.spec.port);
    let proxied_body = match serde_json::to_vec(&body) {
        Ok(body) => body,
        Err(error) => {
            return bad_request_response(format!("failed to encode proxy request body: {error}"))
        }
    };

    let upstream = match state
        .http_client()
        .post(target)
        .header(CONTENT_TYPE, "application/json")
        .body(proxied_body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return json_response(
                502,
                ErrorResponse {
                    error: "server_proxy_failed",
                    message: format!(
                        "failed to proxy chat request to server `{}` at {}:{}: {error}. The server process is recorded as running, but the HTTP target may be unreachable; check `/v1/servers/{}/health`.",
                        server.spec.short_ref,
                        server.spec.host,
                        server.spec.port,
                        server.spec.short_ref
                    ),
                },
            )
        }
    };

    proxy_upstream_response(upstream).await
}

fn chat_server_reference(value: Option<&Value>) -> Result<Option<String>, HttpResponse> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(reference) = value.as_str() else {
        return Err(bad_request_response(
            "`server_ref` must be a string when provided",
        ));
    };
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(bad_request_response(
            "`server_ref` must not be empty when provided",
        ));
    }

    Ok(Some(reference.to_string()))
}

fn select_chat_server(
    manager: &ServerManager,
    reference: Option<&str>,
) -> Result<ServerInspection, HttpResponse> {
    if let Some(reference) = reference {
        let inspection = match manager.inspect(reference) {
            Ok(inspection) => inspection,
            Err(error) => return Err(server_error_response(error)),
        };
        if !inspection.running {
            return Err(json_response(
                409,
                ErrorResponse {
                    error: "server_not_running",
                    message: format!("server `{}` is not running", inspection.spec.short_ref),
                },
            ));
        }
        return Ok(inspection);
    }

    let running = match manager.list_running() {
        Ok(servers) => servers,
        Err(error) => return Err(server_error_response(error)),
    };
    match running.len() {
        0 => Err(json_response(
            409,
            ErrorResponse {
                error: "no_running_server",
                message: "no running server is available for chat proxying".to_string(),
            },
        )),
        1 => manager
            .inspect(&running[0].spec.server_ref)
            .map_err(server_error_response),
        _ => Err(json_response(
            409,
            ErrorResponse {
                error: "ambiguous_server",
                message: "multiple servers are running; provide `server_ref`".to_string(),
            },
        )),
    }
}

async fn proxy_upstream_response(upstream: reqwest::Response) -> HttpResponse {
    let status_code = upstream.status().as_u16();
    let content_type = upstream
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let cache_control = upstream
        .headers()
        .get(CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    if content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/event-stream"))
    {
        return raw_response(
            status_code,
            content_type,
            cache_control,
            HttpBody::Proxy(upstream),
        );
    }

    match upstream.bytes().await {
        Ok(bytes) => raw_response(
            status_code,
            content_type,
            cache_control,
            HttpBody::Buffered(bytes.to_vec()),
        ),
        Err(error) => json_response(
            502,
            ErrorResponse {
                error: "server_proxy_failed",
                message: format!("failed to read proxied chat response: {error}"),
            },
        ),
    }
}
