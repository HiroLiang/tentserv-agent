use serde_json::{json, Value};
use tentgent_core::{
    server::ServerInspection,
    session::{
        SessionAppendOutcome, SessionAppendTurn, SessionCompactionInput, SessionCompactionOutcome,
        SessionCompactionSummary, SessionCompactionTurn,
        SessionCreateRequest as CoreSessionCreateRequest, SessionInspection, SessionManager,
        SessionMessage, SessionMessageInput, SessionMessages, SessionOptionalStringPatch,
        SessionRemovalOutcome, SessionRequestContextSummaryInput, SessionSummary,
        SessionUpdateRequest, SessionWarning,
    },
};

use crate::{
    app::DaemonHttpState,
    dto::{
        ErrorResponse, RemoveSessionResponse, RemovedSessionItem, SessionAppendRequest,
        SessionAppendResponse, SessionAppendSessionItem, SessionAppendedItem,
        SessionCompactRequest, SessionCompactResponse, SessionCompactionItem, SessionCreateRequest,
        SessionCreateResponse, SessionInspectionItem, SessionMessageItem, SessionMessageRequest,
        SessionMessagesResponse, SessionPatchRequest, SessionRefItem, SessionResponse,
        SessionSummaryItem, SessionWarningItem, SessionsResponse,
    },
    http::{HttpRequest, HttpResponse},
    response::{
        bad_request_response, json_response, parse_json_body, session_error_response,
        session_write_error_response,
    },
    routes::{
        chat::{
            chat_context_message_value, chat_server_manager, proxy_upstream_response,
            select_chat_server, send_chat_to_server, upstream_is_event_stream,
        },
        store::path_string,
    },
};

const DEFAULT_TAIL_MESSAGES: usize = 200;
const MAX_TAIL_MESSAGES: usize = 1_000;

pub(crate) fn list_sessions_response(state: &DaemonHttpState) -> HttpResponse {
    let manager = match SessionManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_error_response(error),
    };
    match manager.list() {
        Ok(sessions) => json_response(
            200,
            SessionsResponse {
                sessions: sessions.into_iter().map(session_summary_item).collect(),
            },
        ),
        Err(error) => session_error_response(error),
    }
}

pub(crate) fn inspect_session_response(state: &DaemonHttpState, reference: &str) -> HttpResponse {
    let manager = match SessionManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_error_response(error),
    };
    match manager.inspect(reference) {
        Ok(session) => json_response(
            200,
            SessionResponse {
                session: session_inspection_item(session),
            },
        ),
        Err(error) => session_error_response(error),
    }
}

pub(crate) fn session_messages_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    let tail = match tail_messages(request) {
        Ok(tail) => tail,
        Err(response) => return response,
    };
    let manager = match SessionManager::open_readonly(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_error_response(error),
    };
    match manager.messages(reference, tail) {
        Ok(messages) => json_response(200, session_messages_item(messages)),
        Err(error) => session_error_response(error),
    }
}

pub(crate) fn create_session_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
) -> HttpResponse {
    let body = match parse_json_body::<SessionCreateRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let manager = match SessionManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_write_error_response(error),
    };
    let request = CoreSessionCreateRequest {
        title: body.title,
        default_server_ref: body.default_server_ref,
        adapter_ref: body.adapter_ref,
        tags: body.tags,
        messages: match message_inputs(body.messages) {
            Ok(messages) => messages,
            Err(response) => return response,
        },
    };
    match manager.create(request) {
        Ok(session) => json_response(
            201,
            SessionCreateResponse {
                session: session_inspection_item(session),
                created: true,
            },
        ),
        Err(error) => session_write_error_response(error),
    }
}

pub(crate) fn update_session_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    let body = match parse_json_body::<SessionPatchRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let manager = match SessionManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_write_error_response(error),
    };
    let request = SessionUpdateRequest {
        title: optional_patch(body.title),
        default_server_ref: optional_patch(body.default_server_ref),
        adapter_ref: optional_patch(body.adapter_ref),
        tags: body.tags,
    };
    match manager.update(reference, request) {
        Ok(session) => json_response(
            200,
            SessionResponse {
                session: session_inspection_item(session),
            },
        ),
        Err(error) => session_write_error_response(error),
    }
}

pub(crate) async fn append_session_messages_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    let body = match parse_json_body::<SessionAppendRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let manager = match SessionManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_write_error_response(error),
    };
    let messages = match message_inputs(body.messages) {
        Ok(messages) => messages,
        Err(response) => return response,
    };
    let mut turn = match manager.begin_append_messages(reference, messages) {
        Ok(turn) => turn,
        Err(error) => return session_write_error_response(error),
    };
    if let Some(input) = match turn.compaction_input() {
        Ok(input) => input,
        Err(error) => return session_write_error_response(error),
    } {
        let server_reference = body
            .compaction_server_ref
            .as_deref()
            .or_else(|| append_turn_default_server(&turn));
        let Some(server_reference) = server_reference else {
            return session_write_error_response(
                tentgent_core::session::SessionError::CompactionRequired,
            );
        };
        let server = match resolve_running_server(state, Some(server_reference)) {
            Ok(server) => server,
            Err(response) => return response,
        };
        let summary = match summarize_with_server(state, &server, &input).await {
            Ok(summary) => summary,
            Err(response) => return response,
        };
        if let Err(error) = turn.apply_compaction_summary(summary) {
            return session_write_error_response(error);
        }
    }
    match turn.append_after_compaction() {
        Ok(outcome) => json_response(200, session_append_item(outcome)),
        Err(error) => session_write_error_response(error),
    }
}

pub(crate) async fn compact_session_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    let body = match parse_json_body::<SessionCompactRequest>(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let keep_recent_messages = body.keep_recent_messages.unwrap_or(49);
    if keep_recent_messages > 49 {
        return bad_request_response("`keep_recent_messages` must be between 0 and 49");
    }
    let manager = match SessionManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_write_error_response(error),
    };
    let turn = match manager.begin_compaction(
        reference,
        keep_recent_messages,
        body.instructions.clone(),
    ) {
        Ok(turn) => turn,
        Err(error) => return session_write_error_response(error),
    };
    let input = match turn.compaction_input(body.instructions) {
        Ok(Some(input)) => input,
        Ok(None) => return json_response(200, session_compaction_item(turn.no_op_outcome())),
        Err(error) => return session_write_error_response(error),
    };
    let server_reference = body
        .server_ref
        .as_deref()
        .or_else(|| compaction_turn_default_server(&turn));
    let Some(server_reference) = server_reference else {
        return session_write_error_response(
            tentgent_core::session::SessionError::CompactionRequired,
        );
    };
    let server = match resolve_running_server(state, Some(server_reference)) {
        Ok(server) => server,
        Err(response) => return response,
    };
    let summary = match summarize_with_server(state, &server, &input).await {
        Ok(summary) => summary,
        Err(response) => return response,
    };
    match turn.apply_summary(summary) {
        Ok(outcome) => json_response(200, session_compaction_item(outcome)),
        Err(error) => session_write_error_response(error),
    }
}

pub(crate) fn remove_session_response(
    state: &DaemonHttpState,
    request: &HttpRequest,
    reference: &str,
) -> HttpResponse {
    if !request.body.is_empty() {
        return bad_request_response("DELETE request body must be empty");
    }
    let manager = match SessionManager::new_with_home(Some(state.home_dir())) {
        Ok(manager) => manager,
        Err(error) => return session_write_error_response(error),
    };
    match manager.remove(reference) {
        Ok(outcome) => json_response(200, session_removal_item(outcome)),
        Err(error) => session_write_error_response(error),
    }
}

pub(crate) fn session_messages_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/sessions/")?;
    let reference = rest.strip_suffix("/messages")?;
    if valid_session_reference_path(reference) {
        Some(reference)
    } else {
        None
    }
}

pub(crate) fn session_compact_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/sessions/")?;
    let reference = rest.strip_suffix("/compact")?;
    if valid_session_reference_path(reference) {
        Some(reference)
    } else {
        None
    }
}

pub(crate) fn session_ref_path(path: &str) -> Option<&str> {
    let reference = path.strip_prefix("/v1/sessions/")?;
    if valid_session_reference_path(reference) {
        Some(reference)
    } else {
        None
    }
}

fn append_turn_default_server(turn: &SessionAppendTurn) -> Option<&str> {
    turn.default_server_ref()
}

fn compaction_turn_default_server(turn: &SessionCompactionTurn) -> Option<&str> {
    turn.default_server_ref()
}

fn resolve_running_server(
    state: &DaemonHttpState,
    reference: Option<&str>,
) -> Result<ServerInspection, HttpResponse> {
    let manager = chat_server_manager(state)?;
    select_chat_server(&manager, reference)
}

pub(crate) async fn summarize_with_server(
    state: &DaemonHttpState,
    server: &ServerInspection,
    input: &SessionCompactionInput,
) -> Result<SessionCompactionSummary, HttpResponse> {
    summarize_prompt_with_server(state, server, &input.prompt_messages, "compaction").await
}

pub(crate) async fn summarize_request_context_with_server(
    state: &DaemonHttpState,
    server: &ServerInspection,
    input: &SessionRequestContextSummaryInput,
) -> Result<SessionCompactionSummary, HttpResponse> {
    summarize_prompt_with_server(
        state,
        server,
        &input.prompt_messages,
        "request context summary",
    )
    .await
}

async fn summarize_prompt_with_server(
    state: &DaemonHttpState,
    server: &ServerInspection,
    prompt_messages: &[tentgent_core::session::SessionChatContextMessage],
    label: &str,
) -> Result<SessionCompactionSummary, HttpResponse> {
    let body = json!({
        "messages": prompt_messages
            .iter()
            .map(chat_context_message_value)
            .collect::<Vec<_>>(),
        "stream": false,
    });
    let proxied_body = serde_json::to_vec(&body).map_err(|error| {
        bad_request_response(format!("failed to encode compaction request body: {error}"))
    })?;
    let upstream = send_chat_to_server(state, server, proxied_body).await?;
    if !upstream.status().is_success() {
        return Err(proxy_upstream_response(upstream).await);
    }
    if upstream_is_event_stream(&upstream) {
        return Err(session_compaction_failed_response(format!(
            "{label} target returned SSE but non-stream JSON was required"
        )));
    }
    let payload = match upstream.bytes().await {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).map_err(|error| {
            session_compaction_failed_response(format!(
                "{label} target response was not valid JSON: {error}"
            ))
        })?,
        Err(error) => {
            return Err(json_response(
                502,
                ErrorResponse {
                    error: "server_proxy_failed",
                    message: format!("failed to read compaction target response: {error}"),
                },
            ))
        }
    };
    let Some(text) = payload.get("text").and_then(Value::as_str) else {
        return Err(session_compaction_failed_response(format!(
            "{label} target response did not contain string `text`"
        )));
    };
    if text.trim().is_empty() {
        return Err(session_compaction_failed_response(format!(
            "{label} target returned an empty summary"
        )));
    }

    Ok(SessionCompactionSummary {
        content: text.to_string(),
        server_ref: Some(server.spec.server_ref.clone()),
        model_ref: server.spec.model_ref.clone(),
        provider_model: server.spec.provider_model.clone(),
        adapter_ref: None,
    })
}

fn session_compaction_failed_response(message: impl Into<String>) -> HttpResponse {
    json_response(
        502,
        ErrorResponse {
            error: "session_compaction_failed",
            message: message.into(),
        },
    )
}

pub(crate) fn is_session_route(path: &str) -> bool {
    path == "/v1/sessions" || path.starts_with("/v1/sessions/")
}

fn session_summary_item(summary: SessionSummary) -> SessionSummaryItem {
    let metadata = summary.metadata;
    SessionSummaryItem {
        session_ref: metadata.session_ref,
        short_ref: metadata.short_ref,
        title: metadata.title,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        message_count: metadata.message_count,
        default_server_ref: metadata.default_server_ref,
        adapter_ref: metadata.adapter_ref,
        tags: metadata.tags,
        store_path: path_string(&summary.store_path),
    }
}

fn session_inspection_item(inspection: SessionInspection) -> SessionInspectionItem {
    let metadata = inspection.metadata;
    SessionInspectionItem {
        session_ref: metadata.session_ref,
        short_ref: metadata.short_ref,
        title: metadata.title,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        message_count: metadata.message_count,
        default_server_ref: metadata.default_server_ref,
        adapter_ref: metadata.adapter_ref,
        tags: metadata.tags,
        store_path: path_string(&inspection.store_path),
        messages_path: path_string(&inspection.messages_path),
        warnings: inspection
            .warnings
            .into_iter()
            .map(session_warning_item)
            .collect(),
    }
}

fn session_messages_item(messages: SessionMessages) -> SessionMessagesResponse {
    SessionMessagesResponse {
        session: SessionRefItem {
            session_ref: messages.session_ref,
            short_ref: messages.short_ref,
        },
        messages: messages
            .messages
            .into_iter()
            .map(session_message_item)
            .collect(),
        tail: messages.tail,
        total_messages: messages.total_messages,
        truncated: messages.truncated,
        warnings: messages
            .warnings
            .into_iter()
            .map(session_warning_item)
            .collect(),
    }
}

fn session_append_item(outcome: SessionAppendOutcome) -> SessionAppendResponse {
    SessionAppendResponse {
        session: SessionAppendSessionItem {
            session_ref: outcome.metadata.session_ref,
            short_ref: outcome.metadata.short_ref,
            message_count: outcome.metadata.message_count,
            updated_at: outcome.metadata.updated_at,
        },
        appended: outcome
            .appended
            .into_iter()
            .map(|message| SessionAppendedItem {
                index: message.index,
                role: message.role,
                created_at: message.created_at,
            })
            .collect(),
    }
}

fn session_compaction_item(outcome: SessionCompactionOutcome) -> SessionCompactResponse {
    SessionCompactResponse {
        session: SessionAppendSessionItem {
            session_ref: outcome.metadata.session_ref,
            short_ref: outcome.metadata.short_ref,
            message_count: outcome.metadata.message_count,
            updated_at: outcome.metadata.updated_at,
        },
        compaction: SessionCompactionItem {
            compacted: outcome.compacted,
            source_message_count: outcome.source_message_count,
            replaced_message_count: outcome.replaced_message_count,
            kept_recent_messages: outcome.kept_recent_messages,
            summary_index: outcome.summary_index,
        },
    }
}

fn session_removal_item(outcome: SessionRemovalOutcome) -> RemoveSessionResponse {
    let store_path = path_string(&outcome.inspection.store_path);
    RemoveSessionResponse {
        removed: RemovedSessionItem {
            kind: "session",
            session_ref: outcome.inspection.metadata.session_ref.clone(),
            short_ref: outcome.inspection.metadata.short_ref.clone(),
            store_path,
        },
        session: session_inspection_item(outcome.inspection),
    }
}

fn optional_patch(value: Option<Option<String>>) -> SessionOptionalStringPatch {
    match value {
        None => SessionOptionalStringPatch::Unchanged,
        Some(None) => SessionOptionalStringPatch::Clear,
        Some(Some(value)) => SessionOptionalStringPatch::Set(value),
    }
}

fn message_inputs(
    messages: Vec<SessionMessageRequest>,
) -> Result<Vec<SessionMessageInput>, HttpResponse> {
    messages
        .into_iter()
        .map(|message| {
            if !message.metadata.is_object() {
                return Err(bad_request_response("message metadata must be an object"));
            }
            Ok(SessionMessageInput {
                role: message.role,
                content: message.content,
                server_ref: None,
                adapter_ref: None,
                metadata: message.metadata,
            })
        })
        .collect()
}

fn session_message_item(message: SessionMessage) -> SessionMessageItem {
    SessionMessageItem {
        index: message.index,
        role: message.role,
        content: message.content,
        created_at: message.created_at,
        server_ref: message.server_ref,
        adapter_ref: message.adapter_ref,
        metadata: message.metadata,
    }
}

fn session_warning_item(warning: SessionWarning) -> SessionWarningItem {
    SessionWarningItem {
        code: warning.code,
        message: warning.message,
    }
}

fn tail_messages(request: &HttpRequest) -> Result<usize, HttpResponse> {
    let values = request.query_values("tail").collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(DEFAULT_TAIL_MESSAGES),
        [value] => parse_tail_messages(value),
        _ => Err(bad_request_response("`tail` must be provided at most once")),
    }
}

fn parse_tail_messages(value: &str) -> Result<usize, HttpResponse> {
    let parsed = value.parse::<usize>().map_err(|_| {
        bad_request_response(format!(
            "`tail` must be an integer between 1 and {MAX_TAIL_MESSAGES}"
        ))
    })?;
    if parsed == 0 || parsed > MAX_TAIL_MESSAGES {
        return Err(bad_request_response(format!(
            "`tail` must be between 1 and {MAX_TAIL_MESSAGES}"
        )));
    }
    Ok(parsed)
}

fn valid_session_reference_path(reference: &str) -> bool {
    !reference.is_empty()
        && !reference.contains('/')
        && !reference.contains('\\')
        && !reference.contains("..")
}
