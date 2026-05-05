use std::time::{Duration, Instant};

use reqwest::{header, Method, StatusCode};
use serde_json::Value;

use super::{
    daemon_client::TuiTokenSource,
    navigator::{display_short_ref, percent_encode_path_segment, NavigatorListKind},
};

const SESSION_ACTION_CONNECT_TIMEOUT: Duration = Duration::from_millis(700);
const SESSION_ACTION_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(test)]
pub(super) const SESSION_ACTION_ALLOWED_ROUTES: [&str; 1] = ["DELETE /v1/sessions/{session_ref}"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionActionOrigin {
    Navigator,
    ChatChooseSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionDeleteTarget {
    pub(super) session_ref: String,
    pub(super) short_ref: String,
    pub(super) title: String,
    pub(super) require_full_ref: bool,
    pub(super) origin: SessionActionOrigin,
}

impl SessionDeleteTarget {
    pub(super) fn confirmation_matches(&self, typed: &str) -> bool {
        let typed = typed.trim();
        typed == self.session_ref || (!self.require_full_ref && typed == self.short_ref)
    }

    pub(super) fn confirmation_hint(&self) -> String {
        if self.require_full_ref {
            "Type the full session ref to confirm delete; this short ref is ambiguous".to_string()
        } else {
            format!(
                "Type {} or full ref to confirm permanent session delete",
                self.short_ref
            )
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum SessionActionState {
    Idle,
    ConfirmingDelete {
        target: SessionDeleteTarget,
        typed: String,
        cursor: usize,
        message: String,
    },
    Running {
        request_id: u64,
        generation: u64,
        request: SessionActionRequest,
        started_at: Instant,
    },
    Result(SessionActionResult),
    Error {
        target: Option<SessionDeleteTarget>,
        message: String,
        recoverable: bool,
    },
}

impl Default for SessionActionState {
    fn default() -> Self {
        Self::Idle
    }
}

impl SessionActionState {
    pub(super) fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub(super) fn in_flight(&self) -> Option<u64> {
        match self {
            Self::Running { request_id, .. } => Some(*request_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct SessionActionRequest {
    pub(super) method: Method,
    pub(super) path: String,
    pub(super) body: Option<Value>,
    pub(super) target: SessionDeleteTarget,
}

#[derive(Debug, Clone)]
pub(super) struct SessionActionResult {
    pub(super) status: u16,
    pub(super) lines: Vec<(String, String)>,
    pub(super) raw_summary: String,
    pub(super) target: SessionDeleteTarget,
    pub(super) refresh_targets: Vec<NavigatorListKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SessionActionError {
    AuthRequired(String),
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Timeout(String),
    Down(String),
    Protocol(String),
    Http { status: u16, message: String },
}

impl std::fmt::Display for SessionActionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthRequired(message)
            | Self::BadRequest(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Timeout(message)
            | Self::Down(message)
            | Self::Protocol(message) => formatter.write_str(message),
            Self::Http { status, message } => write!(formatter, "HTTP {status}: {message}"),
        }
    }
}

impl SessionActionError {
    fn from_status(status: StatusCode, path: &str, text: &str) -> Self {
        let message = error_message(text).unwrap_or_else(|| format!("{path} returned {status}"));
        match status {
            StatusCode::UNAUTHORIZED => Self::AuthRequired(message),
            StatusCode::BAD_REQUEST => Self::BadRequest(message),
            StatusCode::NOT_FOUND => Self::NotFound(message),
            StatusCode::CONFLICT => Self::Conflict(message),
            status if status.is_server_error() => Self::Http {
                status: status.as_u16(),
                message,
            },
            status => Self::Http {
                status: status.as_u16(),
                message,
            },
        }
    }
}

pub(super) struct SessionActionClient {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl SessionActionClient {
    pub(super) fn new(
        base_url: String,
        token: Option<String>,
        _token_source: TuiTokenSource,
    ) -> miette::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(SESSION_ACTION_CONNECT_TIMEOUT)
            .build()
            .map_err(|error| miette::miette!("failed to build session action client: {error}"))?;
        Ok(Self {
            base_url,
            token,
            client,
        })
    }

    pub(super) async fn execute(
        &self,
        request: SessionActionRequest,
    ) -> Result<SessionActionResult, SessionActionError> {
        let mut builder = self
            .client
            .request(request.method.clone(), self.endpoint(&request.path))
            .timeout(SESSION_ACTION_REQUEST_TIMEOUT);
        if let Some(token) = self.token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        if let Some(body) = &request.body {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .json(body);
        }
        let response = builder.send().await.map_err(|error| {
            if error.is_timeout() {
                SessionActionError::Timeout(format!("{} timed out: {error}", request.path))
            } else {
                SessionActionError::Down(format!("{} failed: {error}", request.path))
            }
        })?;
        let status = response.status();
        let text = response.text().await.map_err(|error| {
            SessionActionError::Protocol(format!("failed to read response: {error}"))
        })?;
        if !status.is_success() {
            return Err(SessionActionError::from_status(
                status,
                &request.path,
                &text,
            ));
        }
        let value: Value = serde_json::from_str(&text).map_err(|error| {
            SessionActionError::Protocol(format!("invalid {} JSON: {error}", request.path))
        })?;
        Ok(SessionActionResult {
            status: status.as_u16(),
            lines: summarize_session_delete(&value, &request.target),
            raw_summary: bounded_value_summary(&value),
            target: request.target,
            refresh_targets: vec![NavigatorListKind::Sessions],
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }
}

pub(super) fn build_session_delete_request(target: SessionDeleteTarget) -> SessionActionRequest {
    SessionActionRequest {
        method: Method::DELETE,
        path: session_delete_path(&target.session_ref),
        body: None,
        target,
    }
}

pub(super) fn session_delete_path(session_ref: &str) -> String {
    format!("/v1/sessions/{}", percent_encode_path_segment(session_ref))
}

pub(super) fn make_delete_target(
    session_ref: impl Into<String>,
    short_ref: impl Into<String>,
    title: impl Into<String>,
    origin: SessionActionOrigin,
    all_short_refs: impl IntoIterator<Item = String>,
) -> SessionDeleteTarget {
    let session_ref = session_ref.into();
    let short_ref = short_ref.into();
    let short_ref = if short_ref.trim().is_empty() {
        display_short_ref(&session_ref)
    } else {
        short_ref
    };
    let matches = all_short_refs
        .into_iter()
        .filter(|value| value == &short_ref)
        .count();
    SessionDeleteTarget {
        session_ref,
        short_ref,
        title: title.into(),
        require_full_ref: matches > 1,
        origin,
    }
}

fn summarize_session_delete(
    value: &Value,
    fallback: &SessionDeleteTarget,
) -> Vec<(String, String)> {
    let removed = value.get("removed").unwrap_or(value);
    vec![
        ("removed".to_string(), "session".to_string()),
        (
            "short_ref".to_string(),
            string_field(removed, "short_ref")
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| fallback.short_ref.clone()),
        ),
        (
            "session_ref".to_string(),
            string_field(removed, "session_ref")
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| fallback.session_ref.clone()),
        ),
        (
            "title".to_string(),
            if fallback.title.trim().is_empty() {
                "(untitled)".to_string()
            } else {
                fallback.title.clone()
            },
        ),
    ]
}

fn bounded_value_summary(value: &Value) -> String {
    let mut text = value.to_string();
    if text.len() > 600 {
        text.truncate(600);
        text.push_str("...");
    }
    text
}

fn error_message(text: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    string_field(&value, "message")
        .or_else(|| string_field(&value, "error"))
        .map(ToOwned::to_owned)
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_route_percent_encodes_full_ref() {
        let target = make_delete_target(
            "session:abc/def ghi",
            "session:abc/",
            "test",
            SessionActionOrigin::Navigator,
            vec!["session:abc/".to_string()],
        );
        let request = build_session_delete_request(target);

        assert_eq!(request.method, Method::DELETE);
        assert_eq!(request.path, "/v1/sessions/session%3Aabc%2Fdef%20ghi");
        assert!(request.body.is_none());
    }

    #[test]
    fn confirmation_requires_full_ref_when_short_ref_is_ambiguous() {
        let target = make_delete_target(
            "aaaaaaaaaaaa1111",
            "aaaaaaaaaaaa",
            "first",
            SessionActionOrigin::Navigator,
            vec!["aaaaaaaaaaaa".to_string(), "aaaaaaaaaaaa".to_string()],
        );

        assert!(target.require_full_ref);
        assert!(!target.confirmation_matches("aaaaaaaaaaaa"));
        assert!(target.confirmation_matches("aaaaaaaaaaaa1111"));
    }

    #[test]
    fn confirmation_accepts_unique_short_or_full_ref() {
        let target = make_delete_target(
            "bbbbbbbbbbbb1111",
            "bbbbbbbbbbbb",
            "second",
            SessionActionOrigin::ChatChooseSession,
            vec!["bbbbbbbbbbbb".to_string()],
        );

        assert!(target.confirmation_matches("bbbbbbbbbbbb"));
        assert!(target.confirmation_matches("bbbbbbbbbbbb1111"));
    }

    #[test]
    fn action_allowlist_is_session_delete_only() {
        assert_eq!(
            SESSION_ACTION_ALLOWED_ROUTES,
            ["DELETE /v1/sessions/{session_ref}"]
        );
        assert!(!SESSION_ACTION_ALLOWED_ROUTES
            .iter()
            .any(|route| route.contains("/auth") || route.contains("/compact")));
    }
}
