use axum::{
    extract::{Path, RawQuery, State},
    Json,
};
use tentgent_kernel::features::session::usecases::{
    SessionCatalogReadUseCase, SessionInspectRequest, SessionListRequest, SessionMessagesRequest,
};

use crate::transport::rest::{error::RestError, state::RestState};

use super::{
    dto::{
        session_inspection_item, session_messages_item, session_summary_item,
        SessionMessagesResponse, SessionResponse, SessionsResponse,
    },
    parse_selector, session_error, session_store_selection,
};

const DEFAULT_TAIL_MESSAGES: usize = 200;
const MAX_TAIL_MESSAGES: usize = 1_000;

pub async fn list(State(state): State<RestState>) -> Result<Json<SessionsResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .list_sessions(SessionListRequest {
            store: session_store_selection(&state),
        })
        .map_err(session_error)?;

    Ok(Json(SessionsResponse {
        sessions: result
            .sessions
            .into_iter()
            .map(session_summary_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<SessionResponse>, RestError> {
    let selector = parse_selector(&reference)?;
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .inspect_session(SessionInspectRequest {
            store: session_store_selection(&state),
            selector,
        })
        .map_err(session_error)?;

    Ok(Json(SessionResponse {
        session: session_inspection_item(result.inspection),
    }))
}

pub async fn messages(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<SessionMessagesResponse>, RestError> {
    let selector = parse_selector(&reference)?;
    let tail = tail_messages(query.as_deref())?;
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .read_session_messages(SessionMessagesRequest {
            store: session_store_selection(&state),
            selector,
            tail,
        })
        .map_err(session_error)?;

    Ok(Json(session_messages_item(result.messages)))
}

fn tail_messages(query: Option<&str>) -> Result<usize, RestError> {
    let Some(query) = query else {
        return Ok(DEFAULT_TAIL_MESSAGES);
    };
    let values = query
        .split('&')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key == "tail").then_some(value)
        })
        .collect::<Vec<_>>();

    match values.as_slice() {
        [] => Ok(DEFAULT_TAIL_MESSAGES),
        [value] => parse_tail_messages(value),
        _ => Err(RestError::bad_request(
            "bad_request",
            "`tail` must be provided at most once",
        )),
    }
}

fn parse_tail_messages(value: &str) -> Result<usize, RestError> {
    let parsed = value.parse::<usize>().map_err(|_| {
        RestError::bad_request(
            "bad_request",
            format!("`tail` must be an integer between 1 and {MAX_TAIL_MESSAGES}"),
        )
    })?;
    if parsed == 0 || parsed > MAX_TAIL_MESSAGES {
        return Err(RestError::bad_request(
            "bad_request",
            format!("`tail` must be between 1 and {MAX_TAIL_MESSAGES}"),
        ));
    }
    Ok(parsed)
}
