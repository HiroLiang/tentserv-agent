use reqwest::header::{CACHE_CONTROL, CONTENT_TYPE};
use serde_json::{json, Value};
use tentgent_core::{
    adapter::{AdapterError, AdapterManager},
    server::{ServerError, ServerInspection, ServerManager},
    session::{
        SessionChatContextMessage, SessionChatTurn, SessionError, SessionManager,
        SessionMessageInput, DEFAULT_SESSION_CONTEXT_MESSAGES, MAX_MESSAGE_CONTENT_BYTES,
        MAX_SESSION_CONTEXT_MESSAGES,
    },
};
use tokio::sync::mpsc;

use crate::{
    app::DaemonHttpState,
    dto::ErrorResponse,
    http::{HttpBody, HttpRequest, HttpResponse},
    response::{
        bad_request_response, json_response, parse_json_body, raw_response, server_error_response,
        session_write_error_response,
    },
};

const TENTGENT_SSE_CONTENT_TYPE: &str = "text/event-stream; charset=utf-8";

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
    let session_reference =
        match optional_trimmed_string(body_object.get("session_ref"), "session_ref") {
            Ok(reference) => reference,
            Err(response) => return response,
        };
    let max_session_messages = match max_session_messages(
        body_object.get("max_session_messages"),
        session_reference.is_some(),
    ) {
        Ok(value) => value,
        Err(response) => return response,
    };

    body_object.remove("server_ref");
    body_object.remove("session_ref");
    body_object.remove("max_session_messages");

    if session_reference.is_some() {
        return proxy_session_chat_response(
            state,
            body,
            server_reference,
            session_reference.expect("checked session_ref"),
            max_session_messages,
        )
        .await;
    }

    let manager = match chat_server_manager(state) {
        Ok(manager) => manager,
        Err(response) => return response,
    };
    let server = match select_chat_server(&manager, server_reference.as_deref()) {
        Ok(server) => server,
        Err(response) => return response,
    };
    let proxied_body = match serde_json::to_vec(&body) {
        Ok(body) => body,
        Err(error) => {
            return bad_request_response(format!("failed to encode proxy request body: {error}"))
        }
    };

    let upstream = match send_chat_to_server(state, &server, proxied_body).await {
        Ok(response) => response,
        Err(response) => return response,
    };

    proxy_upstream_response(upstream).await
}

async fn proxy_session_chat_response(
    state: &DaemonHttpState,
    mut body: Value,
    request_server_reference: Option<String>,
    session_reference: String,
    max_session_messages: usize,
) -> HttpResponse {
    let Some(body_object) = body.as_object_mut() else {
        return bad_request_response("request body must be a JSON object");
    };
    let request_messages = match session_request_messages(body_object.get("messages")) {
        Ok(messages) => messages,
        Err(response) => return response,
    };

    let session_manager = match SessionManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_write_error_response(error),
    };
    let mut turn = match session_manager.begin_chat_turn(
        &session_reference,
        max_session_messages,
        request_messages,
    ) {
        Ok(turn) => turn,
        Err(error) => return session_write_error_response(error),
    };

    let manager = match chat_server_manager(state) {
        Ok(manager) => manager,
        Err(response) => return response,
    };
    let server_reference = request_server_reference
        .as_deref()
        .or(turn.metadata.default_server_ref.as_deref());
    let server = match select_chat_server_for_session(
        &manager,
        server_reference,
        request_server_reference.is_none() && turn.metadata.default_server_ref.is_some(),
    ) {
        Ok(server) => server,
        Err(response) => return response,
    };
    if let Err(error) = turn.apply_clear_compaction_if_needed() {
        return session_write_error_response(error);
    }
    if let Some(input) = match turn.persisted_compaction_input() {
        Ok(input) => input,
        Err(error) => return session_write_error_response(error),
    } {
        let summary = match super::session::summarize_with_server(state, &server, &input).await {
            Ok(summary) => summary,
            Err(response) => return response,
        };
        if let Err(error) = turn.apply_persisted_compaction_summary(summary) {
            return session_write_error_response(error);
        }
    }
    if let Some(input) = match turn.request_context_summary_input() {
        Ok(input) => input,
        Err(error) => return session_write_error_response(error),
    } {
        let summary =
            match super::session::summarize_request_context_with_server(state, &server, &input)
                .await
            {
                Ok(summary) => summary,
                Err(response) => return response,
            };
        if let Err(error) = turn.apply_request_context_summary(summary) {
            return session_write_error_response(error);
        }
    }

    let adapter_ref = match effective_adapter_ref(
        state,
        body_object.get("adapter_ref"),
        turn.metadata.adapter_ref.as_deref(),
    ) {
        Ok(adapter_ref) => adapter_ref,
        Err(response) => return response,
    };
    if let Some(adapter_ref) = adapter_ref.clone() {
        body_object.insert("adapter_ref".to_string(), Value::String(adapter_ref));
    } else {
        body_object.remove("adapter_ref");
    }
    body_object.insert(
        "messages".to_string(),
        Value::Array(
            turn.context_messages
                .iter()
                .map(chat_context_message_value)
                .collect(),
        ),
    );

    let stream = match optional_bool(body_object.get("stream"), "stream") {
        Ok(value) => value.unwrap_or(false),
        Err(response) => return response,
    };
    let proxied_body = match serde_json::to_vec(&body) {
        Ok(body) => body,
        Err(error) => {
            return bad_request_response(format!("failed to encode proxy request body: {error}"))
        }
    };
    let upstream = match send_chat_to_server(state, &server, proxied_body).await {
        Ok(response) => response,
        Err(response) => return response,
    };

    if stream {
        native_session_stream_response(upstream, server, turn, adapter_ref).await
    } else {
        native_session_non_stream_response(upstream, server, turn, adapter_ref).await
    }
}

pub(crate) fn chat_server_manager(state: &DaemonHttpState) -> Result<ServerManager, HttpResponse> {
    ServerManager::open_readonly(Some(state.home_dir())).map_err(server_error_response)
}

pub(crate) async fn send_chat_to_server(
    state: &DaemonHttpState,
    server: &ServerInspection,
    proxied_body: Vec<u8>,
) -> Result<reqwest::Response, HttpResponse> {
    state
        .http_client()
        .post(chat_target_url(server))
        .header(CONTENT_TYPE, "application/json")
        .body(proxied_body)
        .send()
        .await
        .map_err(|error| chat_proxy_failure(server, error))
}

fn chat_proxy_failure(server: &ServerInspection, error: reqwest::Error) -> HttpResponse {
    json_response(
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

pub(crate) fn chat_target_url(server: &ServerInspection) -> String {
    format!("http://{}:{}/v1/chat", server.spec.host, server.spec.port)
}

pub(crate) fn optional_trimmed_string(
    value: Option<&Value>,
    field: &str,
) -> Result<Option<String>, HttpResponse> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(reference) = value.as_str() else {
        return Err(bad_request_response(format!(
            "`{field}` must be a string when provided"
        )));
    };
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(bad_request_response(format!(
            "`{field}` must not be empty when provided"
        )));
    }

    Ok(Some(reference.to_string()))
}

fn chat_server_reference(value: Option<&Value>) -> Result<Option<String>, HttpResponse> {
    optional_trimmed_string(value, "server_ref")
}

pub(crate) fn max_session_messages(
    value: Option<&Value>,
    has_session_ref: bool,
) -> Result<usize, HttpResponse> {
    let Some(value) = value else {
        return Ok(DEFAULT_SESSION_CONTEXT_MESSAGES);
    };
    if !has_session_ref {
        return Err(bad_request_response(
            "`max_session_messages` requires `session_ref`",
        ));
    }
    let Some(value) = value.as_u64() else {
        return Err(bad_request_response(
            "`max_session_messages` must be an integer when provided",
        ));
    };
    let value = usize::try_from(value)
        .map_err(|_| bad_request_response("`max_session_messages` is too large"))?;
    if value > MAX_SESSION_CONTEXT_MESSAGES {
        return Err(bad_request_response(format!(
            "`max_session_messages` must be at most {MAX_SESSION_CONTEXT_MESSAGES}"
        )));
    }
    Ok(value)
}

pub(crate) fn session_request_messages(
    value: Option<&Value>,
) -> Result<Vec<SessionMessageInput>, HttpResponse> {
    let Some(Value::Array(messages)) = value else {
        return Err(bad_request_response("`messages` must be a non-empty array"));
    };
    if messages.is_empty() {
        return Err(bad_request_response("`messages` must be a non-empty array"));
    }
    messages
        .iter()
        .map(session_request_message)
        .collect::<Result<Vec<_>, _>>()
}

fn session_request_message(value: &Value) -> Result<SessionMessageInput, HttpResponse> {
    let Some(object) = value.as_object() else {
        return Err(bad_request_response("each message must be an object"));
    };
    let role = required_message_string(object.get("role"), "message role must be a string")?;
    if !matches!(role.as_str(), "system" | "user" | "assistant") {
        return Err(bad_request_response(
            "message role must be one of: system, user, assistant",
        ));
    }
    let content =
        required_message_string(object.get("content"), "message content must be a string")?;
    Ok(SessionMessageInput {
        role,
        content,
        server_ref: None,
        adapter_ref: None,
        metadata: json!({}),
    })
}

pub(crate) fn required_message_string(
    value: Option<&Value>,
    message: &str,
) -> Result<String, HttpResponse> {
    let Some(value) = value else {
        return Err(bad_request_response(message));
    };
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(message));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(bad_request_response(message));
    }
    Ok(value.to_string())
}

pub(crate) fn select_chat_server(
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

fn select_chat_server_for_session(
    manager: &ServerManager,
    reference: Option<&str>,
    session_default: bool,
) -> Result<ServerInspection, HttpResponse> {
    if let Some(reference) = reference {
        let inspection = match manager.inspect(reference) {
            Ok(inspection) => inspection,
            Err(ServerError::NotFound(_)) | Err(ServerError::AmbiguousRef(_))
                if session_default =>
            {
                return Err(session_invalid_response(
                    "session default_server_ref no longer resolves to a unique server",
                ))
            }
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
    select_chat_server(manager, None)
}

fn effective_adapter_ref(
    state: &DaemonHttpState,
    request_value: Option<&Value>,
    session_adapter_ref: Option<&str>,
) -> Result<Option<String>, HttpResponse> {
    if let Some(value) = request_value {
        if !value.is_null() {
            return optional_trimmed_string(Some(value), "adapter_ref");
        }
    }
    let Some(adapter_ref) = session_adapter_ref else {
        return Ok(None);
    };
    let manager =
        AdapterManager::open_readonly_with_home(Some(state.home_dir())).map_err(|error| {
            session_invalid_response(format!("failed to resolve session adapter_ref: {error}"))
        })?;
    match manager.inspect(adapter_ref) {
        Ok(inspection) => Ok(Some(inspection.metadata.adapter_ref)),
        Err(AdapterError::NotFound(_)) | Err(AdapterError::AmbiguousRef(_)) => Err(
            session_invalid_response("session adapter_ref no longer resolves to a unique adapter"),
        ),
        Err(error) => Err(session_invalid_response(format!(
            "failed to resolve session adapter_ref: {error}"
        ))),
    }
}

pub(crate) fn optional_bool(
    value: Option<&Value>,
    field: &str,
) -> Result<Option<bool>, HttpResponse> {
    let Some(value) = value else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| bad_request_response(format!("`{field}` must be a boolean when provided")))
}

pub(crate) fn chat_context_message_value(message: &SessionChatContextMessage) -> Value {
    json!({
        "role": message.role,
        "content": message.content,
    })
}

pub(crate) fn assistant_metadata(
    route: &str,
    server: Option<&ServerInspection>,
    adapter_ref: Option<&str>,
    finish_reason: &str,
) -> Value {
    json!({
        "route": route,
        "server_ref": server.map(|server| server.spec.server_ref.clone()),
        "model_ref": server.and_then(|server| server.spec.model_ref.clone()),
        "provider_model": server.and_then(|server| server.spec.provider_model.clone()),
        "adapter_ref": adapter_ref,
        "finish_reason": finish_reason,
    })
}

pub(crate) fn session_invalid_response(message: impl Into<String>) -> HttpResponse {
    json_response(
        409,
        ErrorResponse {
            error: "session_invalid",
            message: message.into(),
        },
    )
}

fn chat_append_failure_response(error: SessionError) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "session_write_failed",
            message: format!("failed to append session transcript: {error}"),
        },
    )
}

async fn native_session_non_stream_response(
    upstream: reqwest::Response,
    server: ServerInspection,
    turn: SessionChatTurn,
    adapter_ref: Option<String>,
) -> HttpResponse {
    let status_code = upstream.status().as_u16();
    let content_type = upstream_content_type(&upstream);
    let cache_control = upstream_cache_control(&upstream);
    if !upstream.status().is_success() {
        return proxy_upstream_response(upstream).await;
    }
    if upstream_is_event_stream(&upstream) {
        return chat_mapping_failed("target chat response was SSE but `stream` was false");
    }
    let bytes = match upstream.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return json_response(
                502,
                ErrorResponse {
                    error: "server_proxy_failed",
                    message: format!("failed to read proxied chat response: {error}"),
                },
            )
        }
    };
    let payload = match serde_json::from_slice::<Value>(&bytes) {
        Ok(payload) => payload,
        Err(error) => {
            return chat_mapping_failed(format!(
                "target chat response was not valid JSON for session recording: {error}"
            ))
        }
    };
    let Some(text) = payload.get("text").and_then(Value::as_str) else {
        return chat_mapping_failed("target chat response could not be recorded in the session");
    };
    if text.is_empty() {
        return chat_mapping_failed("target chat response text was empty");
    }
    let metadata = assistant_metadata("native", Some(&server), adapter_ref.as_deref(), "stop");
    if let Err(error) = turn.append_assistant(
        text.to_string(),
        Some(server.spec.server_ref.clone()),
        adapter_ref,
        metadata,
    ) {
        return chat_append_failure_response(error);
    }

    raw_response(
        status_code,
        content_type,
        cache_control,
        HttpBody::Buffered(bytes.to_vec()),
    )
}

async fn native_session_stream_response(
    upstream: reqwest::Response,
    server: ServerInspection,
    turn: SessionChatTurn,
    adapter_ref: Option<String>,
) -> HttpResponse {
    if !upstream.status().is_success() {
        return proxy_upstream_response(upstream).await;
    }
    if !upstream_is_event_stream(&upstream) {
        return chat_mapping_failed(
            "target chat response was not Server-Sent Events for `stream: true`",
        );
    }

    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        transform_native_session_sse(upstream, sender, server, turn, adapter_ref).await;
    });

    raw_response(
        200,
        TENTGENT_SSE_CONTENT_TYPE,
        Some("no-cache".to_string()),
        HttpBody::Stream(receiver),
    )
}

async fn transform_native_session_sse(
    mut upstream: reqwest::Response,
    sender: mpsc::Sender<Vec<u8>>,
    server: ServerInspection,
    turn: SessionChatTurn,
    adapter_ref: Option<String>,
) {
    let mut buffer = Vec::new();
    let mut assistant = String::new();
    let mut finish_reason = "stop".to_string();
    let mut turn = Some(turn);

    loop {
        match upstream.chunk().await {
            Ok(Some(chunk)) => {
                buffer.extend_from_slice(&chunk);
                while let Some(event_bytes) = next_sse_event(&mut buffer) {
                    let event = match parse_sse_event(&event_bytes) {
                        Ok(event) => event,
                        Err(message) => {
                            let _ =
                                send_native_error(&sender, "server_proxy_failed", &message).await;
                            return;
                        }
                    };
                    match event.event.as_str() {
                        "delta" => {
                            let payload = match serde_json::from_str::<Value>(&event.data) {
                                Ok(payload) => payload,
                                Err(error) => {
                                    let _ = send_native_error(
                                        &sender,
                                        "server_proxy_failed",
                                        &format!("malformed upstream delta event: {error}"),
                                    )
                                    .await;
                                    return;
                                }
                            };
                            let Some(delta) = payload.get("delta").and_then(Value::as_str) else {
                                let _ = send_native_error(
                                    &sender,
                                    "server_proxy_failed",
                                    "upstream delta event did not include a string `delta`",
                                )
                                .await;
                                return;
                            };
                            assistant.push_str(delta);
                            if assistant.len() > MAX_MESSAGE_CONTENT_BYTES {
                                let _ = send_native_error(
                                    &sender,
                                    "session_write_failed",
                                    "failed to append session transcript",
                                )
                                .await;
                                return;
                            }
                            let _ = sender
                                .send(
                                    format!(
                                        "event: delta\ndata: {event_data}\n\n",
                                        event_data = event.data
                                    )
                                    .into_bytes(),
                                )
                                .await;
                        }
                        "done" => {
                            finish_reason = serde_json::from_str::<Value>(&event.data)
                                .ok()
                                .and_then(|payload| {
                                    payload
                                        .get("finish_reason")
                                        .and_then(Value::as_str)
                                        .map(str::to_string)
                                })
                                .unwrap_or_else(|| "stop".to_string());
                            let Some(turn) = turn.take() else {
                                return;
                            };
                            if append_stream_turn(
                                &sender,
                                turn,
                                &server,
                                adapter_ref.as_deref(),
                                &assistant,
                                &finish_reason,
                            )
                            .await
                            .is_ok()
                            {
                                let _ = sender
                                    .send(
                                        format!("event: done\ndata: {}\n\n", event.data)
                                            .into_bytes(),
                                    )
                                    .await;
                            }
                            return;
                        }
                        "error" => {
                            let _ = sender
                                .send(
                                    format!("event: error\ndata: {}\n\n", event.data).into_bytes(),
                                )
                                .await;
                            return;
                        }
                        _ => {}
                    }
                }
            }
            Ok(None) => break,
            Err(error) => {
                let _ = send_native_error(
                    &sender,
                    "server_proxy_failed",
                    &format!("failed to read proxied streaming chat response: {error}"),
                )
                .await;
                return;
            }
        }
    }

    if buffer.iter().any(|byte| !byte.is_ascii_whitespace()) {
        let _ = send_native_error(
            &sender,
            "server_proxy_failed",
            "malformed upstream SSE event",
        )
        .await;
        return;
    }
    let Some(turn) = turn.take() else {
        return;
    };
    if append_stream_turn(
        &sender,
        turn,
        &server,
        adapter_ref.as_deref(),
        &assistant,
        &finish_reason,
    )
    .await
    .is_ok()
    {
        let _ = sender
            .send(b"event: done\ndata: {\"finish_reason\":\"stop\"}\n\n".to_vec())
            .await;
    }
}

async fn append_stream_turn(
    sender: &mpsc::Sender<Vec<u8>>,
    turn: SessionChatTurn,
    server: &ServerInspection,
    adapter_ref: Option<&str>,
    assistant: &str,
    finish_reason: &str,
) -> Result<(), ()> {
    if assistant.is_empty() {
        let _ = send_native_error(
            sender,
            "session_write_failed",
            "failed to append session transcript",
        )
        .await;
        return Err(());
    }
    let metadata = assistant_metadata("native", Some(server), adapter_ref, finish_reason);
    match turn.append_assistant(
        assistant.to_string(),
        Some(server.spec.server_ref.clone()),
        adapter_ref.map(str::to_string),
        metadata,
    ) {
        Ok(_) => Ok(()),
        Err(_) => {
            let _ = send_native_error(
                sender,
                "session_write_failed",
                "failed to append session transcript",
            )
            .await;
            Err(())
        }
    }
}

fn chat_mapping_failed(message: impl Into<String>) -> HttpResponse {
    json_response(
        502,
        ErrorResponse {
            error: "server_proxy_failed",
            message: message.into(),
        },
    )
}

#[derive(Debug)]
pub(crate) struct SseEvent {
    pub(crate) event: String,
    pub(crate) data: String,
}

pub(crate) fn next_sse_event(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    if let Some(position) = find_subslice(buffer, b"\n\n") {
        let event = buffer[..position].to_vec();
        buffer.drain(..position + 2);
        return Some(event);
    }
    if let Some(position) = find_subslice(buffer, b"\r\n\r\n") {
        let event = buffer[..position].to_vec();
        buffer.drain(..position + 4);
        return Some(event);
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(crate) fn parse_sse_event(event_bytes: &[u8]) -> Result<SseEvent, String> {
    let text = std::str::from_utf8(event_bytes)
        .map_err(|error| format!("upstream SSE event was not UTF-8: {error}"))?;
    let mut event = "message".to_string();
    let mut data = Vec::new();

    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("event:") {
            event = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value).to_string());
        }
    }

    Ok(SseEvent {
        event,
        data: data.join("\n"),
    })
}

async fn send_native_error(
    sender: &mpsc::Sender<Vec<u8>>,
    code: &str,
    message: &str,
) -> Result<(), ()> {
    let body = serde_json::to_string(&json!({
        "error": code,
        "message": message,
    }))
    .map_err(|_| ())?;
    sender
        .send(format!("event: error\ndata: {body}\n\n").into_bytes())
        .await
        .map_err(|_| ())
}

pub(crate) async fn proxy_upstream_response(upstream: reqwest::Response) -> HttpResponse {
    let status_code = upstream.status().as_u16();
    let content_type = upstream_content_type(&upstream);
    let cache_control = upstream_cache_control(&upstream);

    if upstream_is_event_stream(&upstream) {
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

pub(crate) fn upstream_is_event_stream(upstream: &reqwest::Response) -> bool {
    upstream_content_type(upstream)
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/event-stream"))
}

pub(crate) fn upstream_content_type(upstream: &reqwest::Response) -> String {
    upstream
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string()
}

pub(crate) fn upstream_cache_control(upstream: &reqwest::Response) -> Option<String> {
    upstream
        .headers()
        .get(CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
