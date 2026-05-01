use tentgent_core::session::{
    SessionInspection, SessionManager, SessionMessage, SessionMessages, SessionSummary,
    SessionWarning,
};

use crate::{
    app::DaemonHttpState,
    dto::{
        SessionInspectionItem, SessionMessageItem, SessionMessagesResponse, SessionRefItem,
        SessionResponse, SessionSummaryItem, SessionWarningItem, SessionsResponse,
    },
    http::{HttpRequest, HttpResponse},
    response::{bad_request_response, json_response, session_error_response},
    routes::store::path_string,
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

pub(crate) fn session_messages_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/v1/sessions/")?;
    let reference = rest.strip_suffix("/messages")?;
    if valid_session_reference_path(reference) {
        Some(reference)
    } else {
        None
    }
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
    !reference.is_empty() && !reference.contains('/')
}
