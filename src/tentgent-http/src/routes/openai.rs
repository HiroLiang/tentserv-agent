use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::StatusCode;
use serde_json::{json, Map, Value};
use tentgent_core::{
    adapter::{AdapterError, AdapterManager},
    server::{ServerError, ServerInspection},
    session::{SessionChatTurn, SessionManager, SessionMessageInput, MAX_MESSAGE_CONTENT_BYTES},
};
use tokio::sync::mpsc;

use crate::{
    app::DaemonHttpState,
    dto::ErrorResponse,
    http::{HttpBody, HttpRequest, HttpResponse},
    response::{
        bad_request_response, json_response, parse_json_body, raw_response, server_error_response,
    },
    routes::chat::{
        assistant_metadata, chat_context_message_value, chat_server_manager, max_session_messages,
        optional_trimmed_string, proxy_upstream_response, required_message_string,
        send_chat_to_server, session_invalid_response, upstream_is_event_stream,
    },
};

const OPENAI_SSE_CONTENT_TYPE: &str = "text/event-stream; charset=utf-8";

#[derive(Debug)]
struct OpenAiChatRequest {
    model: String,
    proxy_body: Value,
    stream: bool,
    session_ref: Option<String>,
    max_session_messages: usize,
    request_messages: Vec<SessionMessageInput>,
    request_adapter_ref: Option<String>,
}

#[derive(Debug, Clone)]
struct OpenAiResponseMetadata {
    id: String,
    created: u64,
    model: String,
}

#[derive(Debug)]
struct SseEvent {
    event: String,
    data: String,
}

enum SseEventOutcome {
    Continue,
    Terminal,
}

pub(crate) async fn chat_completions_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<Value>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let chat_request = match parse_openai_chat_request(body) {
        Ok(request) => request,
        Err(response) => return response,
    };

    let manager = match chat_server_manager(state) {
        Ok(manager) => manager,
        Err(response) => return response,
    };
    let server = match select_openai_chat_server(&manager, &chat_request.model) {
        Ok(server) => server,
        Err(response) => return response,
    };
    let mut proxy_body = chat_request.proxy_body;

    let turn = if let Some(session_ref) = &chat_request.session_ref {
        let session_manager = match SessionManager::new_with_home(Some(state.home_dir())) {
            Ok(manager) => manager,
            Err(error) => return crate::response::session_write_error_response(error),
        };
        let mut turn = match session_manager.begin_chat_turn(
            session_ref,
            chat_request.max_session_messages,
            chat_request.request_messages,
        ) {
            Ok(turn) => turn,
            Err(error) => return crate::response::session_write_error_response(error),
        };
        if let Err(error) = turn.apply_clear_compaction_if_needed() {
            return crate::response::session_write_error_response(error);
        }
        if let Some(input) = match turn.compaction_input() {
            Ok(input) => input,
            Err(error) => return crate::response::session_write_error_response(error),
        } {
            let summary = match super::session::summarize_with_server(state, &server, &input).await
            {
                Ok(summary) => summary,
                Err(response) => return response,
            };
            if let Err(error) = turn.apply_compaction_summary(summary) {
                return crate::response::session_write_error_response(error);
            }
        }
        if let Some(object) = proxy_body.as_object_mut() {
            object.insert(
                "messages".to_string(),
                Value::Array(
                    turn.context_messages
                        .iter()
                        .map(chat_context_message_value)
                        .collect(),
                ),
            );
        }
        Some(turn)
    } else {
        None
    };

    let adapter_ref = match effective_openai_adapter_ref(
        state,
        chat_request.request_adapter_ref.as_deref(),
        turn.as_ref()
            .and_then(|turn| turn.metadata.adapter_ref.as_deref()),
    ) {
        Ok(adapter_ref) => adapter_ref,
        Err(response) => return response,
    };
    if let Some(object) = proxy_body.as_object_mut() {
        if let Some(adapter_ref) = &adapter_ref {
            object.insert(
                "adapter_ref".to_string(),
                Value::String(adapter_ref.clone()),
            );
        } else {
            object.remove("adapter_ref");
        }
    }

    let proxied_body = match serde_json::to_vec(&proxy_body) {
        Ok(body) => body,
        Err(error) => {
            return bad_request_response(format!("failed to encode proxy request body: {error}"))
        }
    };
    let upstream = match send_chat_to_server(state, &server, proxied_body).await {
        Ok(response) => response,
        Err(response) => return response,
    };

    if chat_request.stream {
        if let Some(turn) = turn {
            openai_stream_session_response(upstream, &server, turn, adapter_ref).await
        } else {
            openai_stream_response(upstream, &server).await
        }
    } else if let Some(turn) = turn {
        openai_non_stream_session_response(upstream, &server, turn, adapter_ref).await
    } else {
        openai_non_stream_response(upstream, &server).await
    }
}

fn parse_openai_chat_request(body: Value) -> Result<OpenAiChatRequest, HttpResponse> {
    let Some(object) = body.as_object() else {
        return Err(bad_request_response("request body must be a JSON object"));
    };

    let model = required_trimmed_string(
        object.get("model"),
        "`model` must be a Tentgent server ref or unique prefix in this MVP",
    )?;
    let messages = parse_messages(object.get("messages"))?;
    let stream = parse_optional_bool(object.get("stream"), "stream")?.unwrap_or(false);
    let session_ref = optional_trimmed_string(object.get("session_ref"), "session_ref")?;
    let max_session_messages =
        max_session_messages(object.get("max_session_messages"), session_ref.is_some())?;

    let mut proxy_body = Map::new();
    proxy_body.insert("messages".to_string(), Value::Array(messages.clone()));
    proxy_body.insert("stream".to_string(), Value::Bool(stream));

    if let Some(max_tokens) = object.get("max_tokens") {
        ensure_integer(max_tokens, "max_tokens")?;
        proxy_body.insert("max_tokens".to_string(), max_tokens.clone());
    }
    if let Some(temperature) = object.get("temperature") {
        ensure_number(temperature, "temperature")?;
        proxy_body.insert("temperature".to_string(), temperature.clone());
    }
    let mut request_adapter_ref = None;
    if let Some(adapter_ref) = object.get("adapter_ref") {
        if !adapter_ref.is_null() {
            let adapter_ref =
                required_trimmed_string(Some(adapter_ref), "`adapter_ref` must be a string")?;
            request_adapter_ref = Some(adapter_ref.clone());
            proxy_body.insert("adapter_ref".to_string(), Value::String(adapter_ref));
        }
    }

    Ok(OpenAiChatRequest {
        model,
        proxy_body: Value::Object(proxy_body),
        stream,
        session_ref,
        max_session_messages,
        request_messages: messages
            .iter()
            .map(openai_message_to_session_input)
            .collect::<Result<Vec<_>, _>>()?,
        request_adapter_ref,
    })
}

fn required_trimmed_string(value: Option<&Value>, message: &str) -> Result<String, HttpResponse> {
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

fn parse_messages(value: Option<&Value>) -> Result<Vec<Value>, HttpResponse> {
    let Some(Value::Array(messages)) = value else {
        return Err(bad_request_response("`messages` must be a non-empty array"));
    };
    if messages.is_empty() {
        return Err(bad_request_response("`messages` must be a non-empty array"));
    }

    messages
        .iter()
        .map(parse_message)
        .collect::<Result<Vec<_>, _>>()
}

fn parse_message(value: &Value) -> Result<Value, HttpResponse> {
    let Some(object) = value.as_object() else {
        return Err(bad_request_response("each message must be an object"));
    };
    let role = required_trimmed_string(
        object.get("role"),
        "message role must be one of: system, user, assistant",
    )?;
    if !matches!(role.as_str(), "system" | "user" | "assistant") {
        return Err(bad_request_response(
            "message role must be one of: system, user, assistant",
        ));
    }
    let content =
        required_message_string(object.get("content"), "message content must be a string")?;

    Ok(json!({
        "role": role,
        "content": content,
    }))
}

fn openai_message_to_session_input(value: &Value) -> Result<SessionMessageInput, HttpResponse> {
    let Some(object) = value.as_object() else {
        return Err(bad_request_response("each message must be an object"));
    };
    let role = required_message_string(
        object.get("role"),
        "message role must be one of: system, user, assistant",
    )?;
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

fn parse_optional_bool(value: Option<&Value>, field: &str) -> Result<Option<bool>, HttpResponse> {
    let Some(value) = value else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| bad_request_response(format!("`{field}` must be a boolean when provided")))
}

fn ensure_integer(value: &Value, field: &str) -> Result<(), HttpResponse> {
    if value.as_i64().is_some() || value.as_u64().is_some() {
        Ok(())
    } else {
        Err(bad_request_response(format!(
            "`{field}` must be an integer when provided"
        )))
    }
}

fn ensure_number(value: &Value, field: &str) -> Result<(), HttpResponse> {
    if value.as_f64().is_some() {
        Ok(())
    } else {
        Err(bad_request_response(format!(
            "`{field}` must be a number when provided"
        )))
    }
}

fn select_openai_chat_server(
    manager: &tentgent_core::server::ServerManager,
    model: &str,
) -> Result<ServerInspection, HttpResponse> {
    let inspection = match manager.inspect(model) {
        Ok(inspection) => inspection,
        Err(ServerError::NotFound(_)) => {
            return Err(json_response(
                404,
                ErrorResponse {
                    error: "not_found",
                    message: "`model` must be a Tentgent server ref or unique prefix in this MVP"
                        .to_string(),
                },
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
    Ok(inspection)
}

async fn openai_non_stream_response(
    upstream: reqwest::Response,
    server: &ServerInspection,
) -> HttpResponse {
    if !upstream.status().is_success() {
        return proxy_upstream_response(upstream).await;
    }
    if upstream_is_event_stream(&upstream) {
        return upstream_mapping_failed("target chat response was SSE but `stream` was false");
    }

    let payload = match upstream.bytes().await {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(payload) => payload,
            Err(error) => {
                return upstream_mapping_failed(format!(
                    "target chat response was not valid JSON: {error}"
                ))
            }
        },
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
    let Some(text) = payload.get("text").and_then(Value::as_str) else {
        return upstream_mapping_failed(
            "target chat response could not be mapped to OpenAI-compatible response",
        );
    };

    json_response(
        StatusCode::OK.as_u16(),
        openai_completion_payload(openai_metadata(server), text),
    )
}

async fn openai_non_stream_session_response(
    upstream: reqwest::Response,
    server: &ServerInspection,
    turn: SessionChatTurn,
    adapter_ref: Option<String>,
) -> HttpResponse {
    if !upstream.status().is_success() {
        return proxy_upstream_response(upstream).await;
    }
    if upstream_is_event_stream(&upstream) {
        return upstream_mapping_failed("target chat response was SSE but `stream` was false");
    }

    let payload = match upstream.bytes().await {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(payload) => payload,
            Err(error) => {
                return upstream_mapping_failed(format!(
                    "target chat response was not valid JSON: {error}"
                ))
            }
        },
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
    let Some(text) = payload.get("text").and_then(Value::as_str) else {
        return upstream_mapping_failed(
            "target chat response could not be mapped to OpenAI-compatible response",
        );
    };
    if text.is_empty() {
        return upstream_mapping_failed(
            "target chat response could not be recorded in the session",
        );
    }

    let metadata = assistant_metadata(
        "openai_compat",
        Some(server),
        adapter_ref.as_deref(),
        "stop",
    );
    if let Err(error) = turn.append_assistant(
        text.to_string(),
        Some(server.spec.server_ref.clone()),
        adapter_ref,
        metadata,
    ) {
        return json_response(
            500,
            ErrorResponse {
                error: "session_write_failed",
                message: format!("failed to append session transcript: {error}"),
            },
        );
    }

    json_response(
        StatusCode::OK.as_u16(),
        openai_completion_payload(openai_metadata(server), text),
    )
}

async fn openai_stream_response(
    upstream: reqwest::Response,
    server: &ServerInspection,
) -> HttpResponse {
    if !upstream.status().is_success() {
        return proxy_upstream_response(upstream).await;
    }
    if !upstream_is_event_stream(&upstream) {
        return upstream_mapping_failed(
            "target chat response was not Server-Sent Events for `stream: true`",
        );
    }

    let metadata = openai_metadata(server);
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        transform_tentgent_sse(upstream, sender, metadata).await;
    });

    raw_response(
        200,
        OPENAI_SSE_CONTENT_TYPE,
        Some("no-cache".to_string()),
        HttpBody::Stream(receiver),
    )
}

async fn openai_stream_session_response(
    upstream: reqwest::Response,
    server: &ServerInspection,
    turn: SessionChatTurn,
    adapter_ref: Option<String>,
) -> HttpResponse {
    if !upstream.status().is_success() {
        return proxy_upstream_response(upstream).await;
    }
    if !upstream_is_event_stream(&upstream) {
        return upstream_mapping_failed(
            "target chat response was not Server-Sent Events for `stream: true`",
        );
    }

    let metadata = openai_metadata(server);
    let server = server.clone();
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        transform_tentgent_sse_session(upstream, sender, metadata, server, turn, adapter_ref).await;
    });

    raw_response(
        200,
        OPENAI_SSE_CONTENT_TYPE,
        Some("no-cache".to_string()),
        HttpBody::Stream(receiver),
    )
}

fn effective_openai_adapter_ref(
    state: &DaemonHttpState,
    request_adapter_ref: Option<&str>,
    session_adapter_ref: Option<&str>,
) -> Result<Option<String>, HttpResponse> {
    if let Some(adapter_ref) = request_adapter_ref {
        return Ok(Some(adapter_ref.to_string()));
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

fn openai_completion_payload(metadata: OpenAiResponseMetadata, text: &str) -> Value {
    json!({
        "id": metadata.id,
        "object": "chat.completion",
        "created": metadata.created,
        "model": metadata.model,
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": text
                },
                "finish_reason": "stop"
            }
        ]
    })
}

fn openai_metadata(server: &ServerInspection) -> OpenAiResponseMetadata {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    OpenAiResponseMetadata {
        id: format!("chatcmpl-{}", now.as_nanos()),
        created: now.as_secs(),
        model: server.spec.server_ref.clone(),
    }
}

fn upstream_mapping_failed(message: impl Into<String>) -> HttpResponse {
    json_response(
        502,
        ErrorResponse {
            error: "server_proxy_failed",
            message: message.into(),
        },
    )
}

async fn transform_tentgent_sse(
    mut upstream: reqwest::Response,
    sender: mpsc::Sender<Vec<u8>>,
    metadata: OpenAiResponseMetadata,
) {
    let mut buffer = Vec::new();
    let mut terminal = false;

    loop {
        match upstream.chunk().await {
            Ok(Some(chunk)) => {
                buffer.extend_from_slice(&chunk);
                while let Some(event_bytes) = next_sse_event(&mut buffer) {
                    match handle_sse_event(&sender, &metadata, &event_bytes).await {
                        SseEventOutcome::Continue => {}
                        SseEventOutcome::Terminal => {
                            return;
                        }
                    }
                }
            }
            Ok(None) => break,
            Err(error) => {
                let _ = send_openai_error(
                    &sender,
                    "server_proxy_failed",
                    &format!("failed to read proxied streaming chat response: {error}"),
                )
                .await;
                terminal = true;
                break;
            }
        }
    }

    if !terminal {
        if buffer.iter().any(|byte| !byte.is_ascii_whitespace()) {
            let _ = send_openai_error(
                &sender,
                "server_proxy_failed",
                "malformed upstream SSE event",
            )
            .await;
            return;
        }
        let _ = send_openai_done(&sender, &metadata, "stop").await;
    }
}

async fn transform_tentgent_sse_session(
    mut upstream: reqwest::Response,
    sender: mpsc::Sender<Vec<u8>>,
    metadata: OpenAiResponseMetadata,
    server: ServerInspection,
    turn: SessionChatTurn,
    adapter_ref: Option<String>,
) {
    let mut buffer = Vec::new();
    let mut assistant = String::new();
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
                                send_openai_error(&sender, "server_proxy_failed", &message).await;
                            return;
                        }
                    };
                    match event.event.as_str() {
                        "delta" => {
                            let payload = match serde_json::from_str::<Value>(&event.data) {
                                Ok(payload) => payload,
                                Err(error) => {
                                    let _ = send_openai_error(
                                        &sender,
                                        "server_proxy_failed",
                                        &format!("malformed upstream delta event: {error}"),
                                    )
                                    .await;
                                    return;
                                }
                            };
                            let Some(delta) = payload.get("delta").and_then(Value::as_str) else {
                                let _ = send_openai_error(
                                    &sender,
                                    "server_proxy_failed",
                                    "upstream delta event did not include a string `delta`",
                                )
                                .await;
                                return;
                            };
                            assistant.push_str(delta);
                            if assistant.len() > MAX_MESSAGE_CONTENT_BYTES {
                                let _ = send_openai_error_without_done(
                                    &sender,
                                    "session_write_failed",
                                    "failed to append session transcript",
                                )
                                .await;
                                return;
                            }
                            let _ = send_openai_delta(&sender, &metadata, delta).await;
                        }
                        "done" => {
                            let finish_reason = serde_json::from_str::<Value>(&event.data)
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
                            if append_openai_stream_turn(
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
                                let _ = send_openai_done(&sender, &metadata, &finish_reason).await;
                            }
                            return;
                        }
                        "error" => {
                            let message = serde_json::from_str::<Value>(&event.data)
                                .ok()
                                .and_then(|payload| {
                                    payload
                                        .get("message")
                                        .and_then(Value::as_str)
                                        .map(str::to_string)
                                })
                                .unwrap_or_else(|| {
                                    "upstream streaming chat response returned an error".to_string()
                                });
                            let _ = send_openai_error(&sender, "runtime_error", &message).await;
                            return;
                        }
                        _ => {}
                    }
                }
            }
            Ok(None) => break,
            Err(error) => {
                let _ = send_openai_error(
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
        let _ = send_openai_error(
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
    if append_openai_stream_turn(
        &sender,
        turn,
        &server,
        adapter_ref.as_deref(),
        &assistant,
        "stop",
    )
    .await
    .is_ok()
    {
        let _ = send_openai_done(&sender, &metadata, "stop").await;
    }
}

async fn append_openai_stream_turn(
    sender: &mpsc::Sender<Vec<u8>>,
    turn: SessionChatTurn,
    server: &ServerInspection,
    adapter_ref: Option<&str>,
    assistant: &str,
    finish_reason: &str,
) -> Result<(), ()> {
    if assistant.is_empty() {
        let _ = send_openai_error_without_done(
            sender,
            "session_write_failed",
            "failed to append session transcript",
        )
        .await;
        return Err(());
    }
    let metadata = assistant_metadata("openai_compat", Some(server), adapter_ref, finish_reason);
    match turn.append_assistant(
        assistant.to_string(),
        Some(server.spec.server_ref.clone()),
        adapter_ref.map(str::to_string),
        metadata,
    ) {
        Ok(_) => Ok(()),
        Err(_) => {
            let _ = send_openai_error_without_done(
                sender,
                "session_write_failed",
                "failed to append session transcript",
            )
            .await;
            Err(())
        }
    }
}

async fn handle_sse_event(
    sender: &mpsc::Sender<Vec<u8>>,
    metadata: &OpenAiResponseMetadata,
    event_bytes: &[u8],
) -> SseEventOutcome {
    let event = match parse_sse_event(event_bytes) {
        Ok(event) => event,
        Err(message) => {
            let _ = send_openai_error(sender, "server_proxy_failed", &message).await;
            return SseEventOutcome::Terminal;
        }
    };

    match event.event.as_str() {
        "delta" => {
            let payload = match serde_json::from_str::<Value>(&event.data) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = send_openai_error(
                        sender,
                        "server_proxy_failed",
                        &format!("malformed upstream delta event: {error}"),
                    )
                    .await;
                    return SseEventOutcome::Terminal;
                }
            };
            let Some(delta) = payload.get("delta").and_then(Value::as_str) else {
                let _ = send_openai_error(
                    sender,
                    "server_proxy_failed",
                    "upstream delta event did not include a string `delta`",
                )
                .await;
                return SseEventOutcome::Terminal;
            };
            let _ = send_openai_delta(sender, metadata, delta).await;
            SseEventOutcome::Continue
        }
        "done" => {
            let finish_reason = serde_json::from_str::<Value>(&event.data)
                .ok()
                .and_then(|payload| {
                    payload
                        .get("finish_reason")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "stop".to_string());
            let _ = send_openai_done(sender, metadata, &finish_reason).await;
            SseEventOutcome::Terminal
        }
        "error" => {
            let message = serde_json::from_str::<Value>(&event.data)
                .ok()
                .and_then(|payload| {
                    payload
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| {
                    "upstream streaming chat response returned an error".to_string()
                });
            let _ = send_openai_error(sender, "runtime_error", &message).await;
            SseEventOutcome::Terminal
        }
        _ => SseEventOutcome::Continue,
    }
}

fn next_sse_event(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
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

fn parse_sse_event(event_bytes: &[u8]) -> Result<SseEvent, String> {
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

async fn send_openai_delta(
    sender: &mpsc::Sender<Vec<u8>>,
    metadata: &OpenAiResponseMetadata,
    delta: &str,
) -> Result<(), ()> {
    send_sse_data(
        sender,
        json!({
            "id": metadata.id,
            "object": "chat.completion.chunk",
            "created": metadata.created,
            "model": metadata.model,
            "choices": [
                {
                    "index": 0,
                    "delta": {"content": delta},
                    "finish_reason": null
                }
            ]
        }),
    )
    .await
}

async fn send_openai_done(
    sender: &mpsc::Sender<Vec<u8>>,
    metadata: &OpenAiResponseMetadata,
    finish_reason: &str,
) -> Result<(), ()> {
    send_sse_data(
        sender,
        json!({
            "id": metadata.id,
            "object": "chat.completion.chunk",
            "created": metadata.created,
            "model": metadata.model,
            "choices": [
                {
                    "index": 0,
                    "delta": {},
                    "finish_reason": finish_reason
                }
            ]
        }),
    )
    .await?;
    sender
        .send(b"data: [DONE]\n\n".to_vec())
        .await
        .map_err(|_| ())
}

async fn send_openai_error(
    sender: &mpsc::Sender<Vec<u8>>,
    code: &str,
    message: &str,
) -> Result<(), ()> {
    send_sse_data(
        sender,
        json!({
            "error": {
                "message": message,
                "type": "server_proxy_failed",
                "param": null,
                "code": code
            }
        }),
    )
    .await?;
    sender
        .send(b"data: [DONE]\n\n".to_vec())
        .await
        .map_err(|_| ())
}

async fn send_openai_error_without_done(
    sender: &mpsc::Sender<Vec<u8>>,
    code: &str,
    message: &str,
) -> Result<(), ()> {
    send_sse_data(
        sender,
        json!({
            "error": {
                "message": message,
                "type": code,
                "param": null,
                "code": code
            }
        }),
    )
    .await
}

async fn send_sse_data(sender: &mpsc::Sender<Vec<u8>>, value: Value) -> Result<(), ()> {
    let body = serde_json::to_string(&value).map_err(|_| ())?;
    sender
        .send(format!("data: {body}\n\n").into_bytes())
        .await
        .map_err(|_| ())
}
