use std::time::Duration;

use reqwest::{header, StatusCode};
use serde_json::{json, Value};

use super::{
    daemon_client::TuiTokenSource,
    navigator::{
        display_short_ref, percent_encode_path_segment, NavigatorRow, SESSION_MESSAGES_TAIL,
    },
};

pub(super) const CHAT_MESSAGES_TAIL: usize = SESSION_MESSAGES_TAIL;
pub(super) const CHAT_MAX_TOKENS: usize = 512;
pub(super) const CHAT_TEMPERATURE: f64 = 0.0;
const CHAT_CONNECT_TIMEOUT: Duration = Duration::from_millis(700);
const CHAT_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(test)]
pub(super) const CHAT_ALLOWED_GET_ROUTES: [&str; 4] = [
    "/v1/servers",
    "/v1/sessions",
    "/v1/sessions/{ref}",
    "/v1/sessions/{ref}/messages?tail=50",
];
#[cfg(test)]
pub(super) const CHAT_ALLOWED_POST_ROUTES: [&str; 2] = ["/v1/sessions", "/v1/chat"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChatPhase {
    NoRunningServer,
    ChooseServer,
    ChooseSession,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChatContextMode {
    None,
    Last2,
    Last10,
    Last50,
}

impl ChatContextMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Last2 => "last 2",
            Self::Last10 => "last 10",
            Self::Last50 => "last 50",
        }
    }

    pub(super) fn max_session_messages(self) -> usize {
        match self {
            Self::None => 0,
            Self::Last2 => 2,
            Self::Last10 => 10,
            Self::Last50 => 50,
        }
    }

    pub(super) fn next(self) -> Self {
        match self {
            Self::None => Self::Last2,
            Self::Last2 => Self::Last10,
            Self::Last10 => Self::Last50,
            Self::Last50 => Self::None,
        }
    }
}

impl Default for ChatContextMode {
    fn default() -> Self {
        Self::Last2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChatFocus {
    Chooser,
    Transcript,
    Composer,
}

impl ChatFocus {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Chooser => "chooser",
            Self::Transcript => "transcript",
            Self::Composer => "composer",
        }
    }

    pub(super) fn next(self) -> Self {
        match self {
            Self::Chooser => Self::Transcript,
            Self::Transcript => Self::Composer,
            Self::Composer => Self::Chooser,
        }
    }
}

impl ChatPhase {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::NoRunningServer => "no running server",
            Self::ChooseServer => "choose server",
            Self::ChooseSession => "choose session",
            Self::Workspace => "workspace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChatSendState {
    Idle,
    CreatingSession { request_id: u64 },
    Sending { request_id: u64 },
    Streaming { request_id: u64 },
    RefreshingAfterSend { request_id: u64 },
    Error,
}

impl ChatSendState {
    pub(super) fn label(&self) -> String {
        match self {
            Self::Idle => "idle".to_string(),
            Self::CreatingSession { .. } => "creating session".to_string(),
            Self::Sending { .. } => "sending".to_string(),
            Self::Streaming { .. } => "streaming".to_string(),
            Self::RefreshingAfterSend { .. } => "refreshing after send".to_string(),
            Self::Error => "error".to_string(),
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle | Self::Error)
    }

    pub(super) fn is_in_flight(&self) -> bool {
        matches!(
            self,
            Self::CreatingSession { .. }
                | Self::Sending { .. }
                | Self::Streaming { .. }
                | Self::RefreshingAfterSend { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChatLoadState {
    Idle,
    Loading { request_id: u64 },
    Ready,
    Error { message: String, stale: bool },
    StaleSelection { message: String },
}

impl ChatLoadState {
    pub(super) fn label(&self) -> String {
        match self {
            Self::Idle => "not loaded".to_string(),
            Self::Loading { .. } => "loading".to_string(),
            Self::Ready => "ready".to_string(),
            Self::Error { message, stale } => {
                if *stale {
                    format!("stale; {message}")
                } else {
                    message.clone()
                }
            }
            Self::StaleSelection { message } => message.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct ChatState {
    pub(super) phase: ChatPhase,
    pub(super) focus: ChatFocus,
    pub(super) load_state: ChatLoadState,
    pub(super) servers: Vec<ChatServerRow>,
    pub(super) sessions: Vec<ChatSessionRow>,
    pub(super) adapters: Vec<ChatAdapterRow>,
    pub(super) selected_server: usize,
    pub(super) selected_session: usize,
    pub(super) selected_adapter: Option<usize>,
    pub(super) selected_server_ref: Option<String>,
    pub(super) selected_session_ref: Option<String>,
    pub(super) selected_adapter_ref: Option<String>,
    pub(super) context_mode: ChatContextMode,
    pub(super) transcript: Vec<ChatMessageRow>,
    pub(super) transcript_scroll_offset: usize,
    pub(super) total_messages: Option<usize>,
    pub(super) transcript_truncated: bool,
    pub(super) composer: String,
    pub(super) composer_cursor: usize,
    pub(super) pending_user: Option<String>,
    pub(super) pending_assistant: Option<String>,
    pub(super) pending_interrupted: bool,
    pub(super) send_state: ChatSendState,
    pub(super) last_error: Option<String>,
    pub(super) retry_non_stream: Option<ChatSendRequest>,
}

impl ChatState {
    pub(super) fn reset_runtime(&mut self) {
        self.load_state = ChatLoadState::Idle;
        self.focus = ChatFocus::Chooser;
        self.servers.clear();
        self.sessions.clear();
        self.adapters.clear();
        self.selected_server = 0;
        self.selected_session = 0;
        self.selected_adapter = None;
        self.selected_server_ref = None;
        self.selected_session_ref = None;
        self.selected_adapter_ref = None;
        self.transcript.clear();
        self.transcript_scroll_offset = 0;
        self.total_messages = None;
        self.transcript_truncated = false;
        self.pending_user = None;
        self.pending_assistant = None;
        self.pending_interrupted = false;
        self.send_state = ChatSendState::Idle;
        self.last_error = None;
        self.retry_non_stream = None;
    }

    pub(super) fn apply_overview(
        &mut self,
        servers: Vec<ChatServerRow>,
        sessions: Vec<ChatSessionRow>,
        adapters: Vec<ChatAdapterRow>,
    ) {
        self.servers = servers;
        self.sessions = sessions;
        self.adapters = adapters;
        self.selected_server = clamp_to_len(self.selected_server, self.servers.len());
        self.selected_session = clamp_to_len(self.selected_session, self.sessions.len() + 1);
        self.selected_adapter = self
            .selected_adapter
            .and_then(|index| (index < self.adapters.len()).then_some(index));
        self.selected_server_ref = self
            .selected_server_ref
            .clone()
            .filter(|selected| self.servers.iter().any(|row| row.server_ref == *selected));
        self.selected_session_ref = self
            .selected_session_ref
            .clone()
            .filter(|selected| self.sessions.iter().any(|row| row.session_ref == *selected));
        if self.selected_server_ref.is_none() {
            if let Some(session_ref) = &self.selected_session_ref {
                self.selected_server_ref = self
                    .sessions
                    .iter()
                    .find(|row| row.session_ref == *session_ref)
                    .and_then(|row| row.default_server_ref.clone())
                    .filter(|server_ref| {
                        self.servers.iter().any(|row| row.server_ref == *server_ref)
                    });
            }
        }
        if self.selected_server_ref.is_none() && self.selected_session_ref.is_none() {
            self.selected_server_ref = self
                .servers
                .get(self.selected_server)
                .map(|row| row.server_ref.clone());
        }
        self.selected_server = self
            .selected_server_ref
            .as_deref()
            .and_then(|selected| {
                self.servers
                    .iter()
                    .position(|row| row.server_ref == selected)
            })
            .unwrap_or(0);
        self.selected_adapter_ref = self
            .selected_adapter
            .and_then(|index| self.adapters.get(index))
            .map(|row| row.adapter_ref.clone());
        self.load_state = ChatLoadState::Ready;
        self.recompute_phase();
    }

    pub(super) fn recompute_phase(&mut self) {
        if self.servers.is_empty() {
            self.phase = ChatPhase::NoRunningServer;
            self.focus = ChatFocus::Chooser;
        } else if self.selected_server_ref.is_none() {
            self.phase = ChatPhase::ChooseServer;
            self.focus = ChatFocus::Chooser;
        } else if self.selected_session_ref.is_none() {
            self.phase = ChatPhase::ChooseSession;
            self.focus = ChatFocus::Chooser;
        } else {
            self.phase = ChatPhase::Workspace;
            if self.focus == ChatFocus::Chooser {
                self.focus = ChatFocus::Composer;
            }
        }
    }

    pub(super) fn selected_server_row(&self) -> Option<&ChatServerRow> {
        let selected = self.selected_server_ref.as_deref()?;
        self.servers.iter().find(|row| row.server_ref == selected)
    }

    pub(super) fn selected_session_row(&self) -> Option<&ChatSessionRow> {
        let selected = self.selected_session_ref.as_deref()?;
        self.sessions.iter().find(|row| row.session_ref == selected)
    }

    pub(super) fn selected_adapter_row(&self) -> Option<&ChatAdapterRow> {
        let selected = self.selected_adapter_ref.as_deref()?;
        self.adapters.iter().find(|row| row.adapter_ref == selected)
    }

    pub(super) fn cycle_context_mode(&mut self) {
        self.context_mode = self.context_mode.next();
    }

    pub(super) fn long_context_warning(&self) -> bool {
        self.context_mode == ChatContextMode::Last50
            && self.total_messages.unwrap_or(self.transcript.len()) >= 20
    }

    pub(super) fn greeting_loop_warning(&self) -> bool {
        let mut hello_count = 0usize;
        let mut hi_count = 0usize;
        let mut zh_hello_count = 0usize;
        for message in &self.transcript {
            if message.role != "assistant" {
                continue;
            }
            match greeting_prefix(&message.content) {
                Some("hello") => hello_count += 1,
                Some("hi") => hi_count += 1,
                Some("zh_hello") => zh_hello_count += 1,
                _ => {}
            }
        }
        hello_count >= 2 || hi_count >= 2 || zh_hello_count >= 2
    }

    pub(super) fn select_server_by_index(&mut self, index: usize) {
        if let Some(row) = self.servers.get(index) {
            self.selected_server = index;
            self.selected_server_ref = Some(row.server_ref.clone());
            if let Some(session) = self.selected_session_row() {
                if session
                    .default_server_ref
                    .as_deref()
                    .is_some_and(|server_ref| server_ref != row.server_ref)
                {
                    self.selected_session_ref = None;
                    self.selected_session = 0;
                }
            }
        }
        self.recompute_phase();
    }

    pub(super) fn select_session_by_index(&mut self, index: usize) {
        self.selected_session = clamp_to_len(index, self.sessions.len() + 1);
        if self.selected_session == 0 {
            self.selected_session_ref = None;
        } else if let Some(row) = self.sessions.get(self.selected_session - 1) {
            self.selected_session_ref = Some(row.session_ref.clone());
        }
        self.recompute_phase();
    }

    pub(super) fn select_adapter_next(&mut self) {
        if self.adapters.is_empty() {
            self.selected_adapter = None;
            self.selected_adapter_ref = None;
            return;
        }
        let next = self
            .selected_adapter
            .map(|index| (index + 1) % (self.adapters.len() + 1))
            .unwrap_or(0);
        if next >= self.adapters.len() {
            self.selected_adapter = None;
            self.selected_adapter_ref = None;
        } else {
            self.selected_adapter = Some(next);
            self.selected_adapter_ref = Some(self.adapters[next].adapter_ref.clone());
        }
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        match self.phase {
            ChatPhase::NoRunningServer | ChatPhase::Workspace => {}
            ChatPhase::ChooseServer => {
                self.selected_server = move_index(self.selected_server, self.servers.len(), delta);
                self.selected_server_ref = self
                    .servers
                    .get(self.selected_server)
                    .map(|row| row.server_ref.clone());
            }
            ChatPhase::ChooseSession => {
                self.selected_session =
                    move_index(self.selected_session, self.sessions.len() + 1, delta);
            }
        }
    }

    pub(super) fn scroll_transcript(&mut self, delta: isize) {
        if delta < 0 {
            self.transcript_scroll_offset = self.transcript_scroll_offset.saturating_sub(
                delta
                    .checked_abs()
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(usize::MAX),
            );
        } else {
            self.transcript_scroll_offset = self
                .transcript_scroll_offset
                .saturating_add(usize::try_from(delta).unwrap_or(usize::MAX));
        }
    }

    pub(super) fn scroll_transcript_to_top(&mut self) {
        self.transcript_scroll_offset = usize::MAX;
    }

    pub(super) fn scroll_transcript_to_bottom(&mut self) {
        self.transcript_scroll_offset = 0;
    }

    pub(super) fn apply_messages(&mut self, messages: ChatMessages) {
        self.transcript = messages.messages;
        self.transcript_scroll_offset = 0;
        self.total_messages = Some(messages.total_messages);
        self.transcript_truncated = messages.truncated;
        self.pending_user = None;
        self.pending_assistant = None;
        self.pending_interrupted = false;
        self.send_state = ChatSendState::Idle;
        self.retry_non_stream = None;
        self.last_error = None;
    }

    pub(super) fn start_pending_send(&mut self, request_id: u64, prompt: String) {
        self.pending_user = Some(prompt);
        self.pending_assistant = Some(String::new());
        self.pending_interrupted = false;
        self.transcript_scroll_offset = 0;
        self.send_state = ChatSendState::Streaming { request_id };
        self.last_error = None;
        self.retry_non_stream = None;
    }

    pub(super) fn append_delta(&mut self, delta: &str) {
        let pending = self.pending_assistant.get_or_insert_with(String::new);
        pending.push_str(delta);
    }
}

impl Default for ChatPhase {
    fn default() -> Self {
        Self::NoRunningServer
    }
}

impl Default for ChatFocus {
    fn default() -> Self {
        Self::Chooser
    }
}

impl Default for ChatLoadState {
    fn default() -> Self {
        Self::Idle
    }
}

impl Default for ChatSendState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone)]
pub(super) struct ChatOverview {
    pub(super) servers: Vec<ChatServerRow>,
    pub(super) sessions: Vec<ChatSessionRow>,
}

#[derive(Debug, Clone)]
pub(super) struct ChatServerRow {
    pub(super) server_ref: String,
    pub(super) short_ref: String,
    pub(super) label: String,
    pub(super) running: bool,
    pub(super) host: Option<String>,
    pub(super) port: Option<u64>,
    pub(super) model: Option<String>,
    pub(super) raw: Value,
}

#[derive(Debug, Clone)]
pub(super) struct ChatSessionRow {
    pub(super) session_ref: String,
    pub(super) short_ref: String,
    pub(super) title: String,
    pub(super) message_count: Option<usize>,
    pub(super) updated_at: Option<String>,
    pub(super) default_server_ref: Option<String>,
    pub(super) adapter_ref: Option<String>,
    pub(super) raw: Value,
}

#[derive(Debug, Clone)]
pub(super) struct ChatAdapterRow {
    pub(super) adapter_ref: String,
    pub(super) short_ref: String,
    pub(super) label: String,
    pub(super) raw: Value,
}

#[derive(Debug, Clone)]
pub(super) struct ChatMessages {
    pub(super) messages: Vec<ChatMessageRow>,
    pub(super) total_messages: usize,
    pub(super) truncated: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ChatMessageRow {
    pub(super) index: Option<usize>,
    pub(super) role: String,
    pub(super) content: String,
    pub(super) created_at: Option<String>,
    pub(super) server_ref: Option<String>,
    pub(super) adapter_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChatSendRequest {
    pub(super) request_id: u64,
    pub(super) server_ref: String,
    pub(super) session_ref: String,
    pub(super) adapter_ref: Option<String>,
    pub(super) prompt: String,
    pub(super) context_mode: ChatContextMode,
    pub(super) max_session_messages: usize,
    pub(super) stream: bool,
}

impl ChatSendRequest {
    pub(super) fn body(&self) -> Value {
        let mut body = json!({
            "server_ref": self.server_ref,
            "session_ref": self.session_ref,
            "max_session_messages": self.max_session_messages,
            "messages": [{"role": "user", "content": self.prompt}],
            "max_tokens": CHAT_MAX_TOKENS,
            "temperature": CHAT_TEMPERATURE,
            "stream": self.stream,
        });
        if let Some(adapter_ref) = &self.adapter_ref {
            body["adapter_ref"] = Value::String(adapter_ref.clone());
        }
        body
    }

    pub(super) fn non_stream(&self) -> Self {
        let mut request = self.clone();
        request.stream = false;
        request
    }

    pub(super) fn with_request_id(mut self, request_id: u64) -> Self {
        self.request_id = request_id;
        self
    }
}

fn greeting_prefix(content: &str) -> Option<&'static str> {
    let trimmed =
        content.trim_start_matches(|ch: char| ch.is_whitespace() || ch.is_ascii_punctuation());
    let normalized = trimmed.to_lowercase();
    if normalized.starts_with("hello") {
        Some("hello")
    } else if normalized.starts_with("hi") || normalized.starts_with("hey") {
        Some("hi")
    } else if trimmed.starts_with("\u{4f60}\u{597d}") {
        Some("zh_hello")
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChatConflictKind {
    SessionBusy,
    ServerStopped,
    MultipleRunningServers,
    NoRunningServer,
    CompactionRequired,
    CompactionFailed,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChatError {
    AuthRequired(String),
    Down(String),
    NotFound(String),
    Conflict {
        kind: ChatConflictKind,
        message: String,
    },
    StreamUnsupported(String),
    ServerProxyFailed(String),
    Timeout(String),
    Protocol(String),
    Server(String),
    Http {
        status: u16,
        message: String,
    },
}

impl ChatError {
    pub(super) fn is_auth_required(&self) -> bool {
        matches!(self, Self::AuthRequired(_))
    }

    pub(super) fn is_down(&self) -> bool {
        matches!(self, Self::Down(_) | Self::Timeout(_))
    }

    pub(super) fn is_stream_unsupported(&self) -> bool {
        matches!(self, Self::StreamUnsupported(_))
    }

    fn from_transport(path: &str, error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout(format!("{path} timed out: {error}"))
        } else {
            Self::Down(format!("{path} failed: {error}"))
        }
    }
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthRequired(message)
            | Self::Down(message)
            | Self::NotFound(message)
            | Self::StreamUnsupported(message)
            | Self::ServerProxyFailed(message)
            | Self::Timeout(message)
            | Self::Protocol(message)
            | Self::Server(message) => formatter.write_str(message),
            Self::Conflict { message, .. } => formatter.write_str(message),
            Self::Http { message, .. } => formatter.write_str(message),
        }
    }
}

pub(super) struct ChatClient {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl ChatClient {
    pub(super) fn new(
        base_url: String,
        token: Option<String>,
        _token_source: TuiTokenSource,
    ) -> miette::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(CHAT_CONNECT_TIMEOUT)
            .build()
            .map_err(|error| miette::miette!("failed to build chat client: {error}"))?;
        Ok(Self {
            base_url,
            token,
            client,
        })
    }

    pub(super) async fn overview(&self) -> Result<ChatOverview, ChatError> {
        let servers = self.list_servers().await?;
        let sessions = self.list_sessions().await?;
        Ok(ChatOverview { servers, sessions })
    }

    pub(super) async fn list_servers(&self) -> Result<Vec<ChatServerRow>, ChatError> {
        let value = self.get_json_value("/v1/servers").await?;
        parse_servers(value)
    }

    pub(super) async fn list_sessions(&self) -> Result<Vec<ChatSessionRow>, ChatError> {
        let value = self.get_json_value("/v1/sessions").await?;
        parse_sessions(value)
    }

    pub(super) async fn inspect_session(
        &self,
        session_ref: &str,
    ) -> Result<ChatSessionRow, ChatError> {
        let path = chat_session_path(session_ref);
        let value = self.get_json_value(&path).await?;
        parse_session(value)
    }

    pub(super) async fn session_messages(
        &self,
        session_ref: &str,
    ) -> Result<ChatMessages, ChatError> {
        let path = chat_messages_path(session_ref);
        let value = self.get_json_value(&path).await?;
        parse_messages(value)
    }

    pub(super) async fn create_session(
        &self,
        title: String,
        server_ref: String,
        adapter_ref: Option<String>,
    ) -> Result<ChatSessionRow, ChatError> {
        let body = session_create_body(title, server_ref, adapter_ref);
        let value = self.post_json_value("/v1/sessions", body).await?;
        parse_created_session(value)
    }

    pub(super) async fn post_non_stream(
        &self,
        request: &ChatSendRequest,
    ) -> Result<String, ChatError> {
        let value = self.post_json_value("/v1/chat", request.body()).await?;
        value
            .get("text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ChatError::Protocol("non-stream chat response missing string `text`".to_string())
            })
    }

    pub(super) async fn post_stream(
        &self,
        request: &ChatSendRequest,
    ) -> Result<reqwest::Response, ChatError> {
        let response = self
            .post("/v1/chat")
            .json(&request.body())
            .send()
            .await
            .map_err(|error| ChatError::from_transport("/v1/chat", error))?;
        if !response.status().is_success() {
            return Err(chat_error_from_response(response, "/v1/chat").await);
        }
        let is_sse = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        if !is_sse {
            return Err(ChatError::StreamUnsupported(
                "chat stream response was not Server-Sent Events".to_string(),
            ));
        }
        Ok(response)
    }

    async fn get_json_value(&self, path: &str) -> Result<Value, ChatError> {
        let response = self
            .get(path)
            .timeout(CHAT_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| ChatError::from_transport(path, error))?;
        if !response.status().is_success() {
            return Err(chat_error_from_response(response, path).await);
        }
        response
            .json::<Value>()
            .await
            .map_err(|error| ChatError::Protocol(format!("invalid {path} JSON: {error}")))
    }

    async fn post_json_value(&self, path: &str, body: Value) -> Result<Value, ChatError> {
        let response = self
            .post(path)
            .timeout(CHAT_REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|error| ChatError::from_transport(path, error))?;
        if !response.status().is_success() {
            return Err(chat_error_from_response(response, path).await);
        }
        response
            .json::<Value>()
            .await
            .map_err(|error| ChatError::Protocol(format!("invalid {path} JSON: {error}")))
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.get(self.endpoint(path));
        self.authorize(request)
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.post(self.endpoint(path));
        self.authorize(request)
    }

    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.token.as_deref() {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }
}

pub(super) fn chat_session_path(session_ref: &str) -> String {
    format!("/v1/sessions/{}", percent_encode_path_segment(session_ref))
}

pub(super) fn chat_messages_path(session_ref: &str) -> String {
    format!(
        "/v1/sessions/{}/messages?tail={}",
        percent_encode_path_segment(session_ref),
        CHAT_MESSAGES_TAIL
    )
}

pub(super) fn session_create_body(
    title: String,
    server_ref: String,
    adapter_ref: Option<String>,
) -> Value {
    let mut body = json!({
        "title": title,
        "default_server_ref": server_ref,
        "tags": [],
        "messages": [],
    });
    if let Some(adapter_ref) = adapter_ref {
        body["adapter_ref"] = Value::String(adapter_ref);
    }
    body
}

pub(super) fn adapter_rows_from_navigator(rows: &[NavigatorRow]) -> Vec<ChatAdapterRow> {
    rows.iter()
        .map(|row| ChatAdapterRow {
            adapter_ref: row.item_ref.clone(),
            short_ref: row.short_ref.clone(),
            label: first_nonempty(&[
                string_field(&row.raw, "name"),
                string_field(&row.raw, "type"),
                string_field(&row.raw, "format"),
            ])
            .unwrap_or(&row.short_ref)
            .to_string(),
            raw: row.raw.clone(),
        })
        .collect()
}

pub(super) fn parse_servers(value: Value) -> Result<Vec<ChatServerRow>, ChatError> {
    let servers = value
        .get("servers")
        .and_then(Value::as_array)
        .ok_or_else(|| ChatError::Protocol("missing `servers` array".to_string()))?;
    Ok(servers
        .iter()
        .filter_map(|server| {
            let server_ref = string_field(server, "server_ref")?.to_string();
            let running = server
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            running.then(|| ChatServerRow {
                short_ref: string_field(server, "short_ref")
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| display_short_ref(&server_ref)),
                label: first_nonempty(&[
                    string_field(server, "provider_model"),
                    string_field(server, "model_ref"),
                    string_field(server, "runtime_kind"),
                ])
                .unwrap_or(&server_ref)
                .to_string(),
                server_ref,
                running,
                host: string_field(server, "host").map(ToOwned::to_owned),
                port: server.get("port").and_then(Value::as_u64),
                model: first_nonempty(&[
                    string_field(server, "model_ref"),
                    string_field(server, "provider_model"),
                ])
                .map(ToOwned::to_owned),
                raw: server.clone(),
            })
        })
        .collect())
}

pub(super) fn parse_sessions(value: Value) -> Result<Vec<ChatSessionRow>, ChatError> {
    let sessions = value
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or_else(|| ChatError::Protocol("missing `sessions` array".to_string()))?;
    Ok(sessions.iter().filter_map(session_row).collect())
}

pub(super) fn parse_session(value: Value) -> Result<ChatSessionRow, ChatError> {
    let session = value
        .get("session")
        .ok_or_else(|| ChatError::Protocol("missing `session` object".to_string()))?;
    session_row(session).ok_or_else(|| ChatError::Protocol("session missing ref".to_string()))
}

fn parse_created_session(value: Value) -> Result<ChatSessionRow, ChatError> {
    parse_session(value)
}

pub(super) fn parse_messages(value: Value) -> Result<ChatMessages, ChatError> {
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| ChatError::Protocol("missing `messages` array".to_string()))?;
    let total_messages = value
        .get("total_messages")
        .and_then(Value::as_u64)
        .unwrap_or(messages.len() as u64) as usize;
    let truncated = value
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(ChatMessages {
        messages: messages
            .iter()
            .map(|message| ChatMessageRow {
                index: message
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize),
                role: string_field(message, "role")
                    .unwrap_or("message")
                    .to_string(),
                content: string_field(message, "content").unwrap_or("").to_string(),
                created_at: string_field(message, "created_at").map(ToOwned::to_owned),
                server_ref: string_field(message, "server_ref").map(ToOwned::to_owned),
                adapter_ref: string_field(message, "adapter_ref").map(ToOwned::to_owned),
            })
            .collect(),
        total_messages,
        truncated,
    })
}

fn session_row(value: &Value) -> Option<ChatSessionRow> {
    let session_ref = string_field(value, "session_ref")?.to_string();
    Some(ChatSessionRow {
        short_ref: string_field(value, "short_ref")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| display_short_ref(&session_ref)),
        title: string_field(value, "title")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("(untitled)")
            .to_string(),
        message_count: value
            .get("message_count")
            .and_then(Value::as_u64)
            .map(|value| value as usize),
        updated_at: string_field(value, "updated_at").map(ToOwned::to_owned),
        default_server_ref: string_field(value, "default_server_ref").map(ToOwned::to_owned),
        adapter_ref: string_field(value, "adapter_ref").map(ToOwned::to_owned),
        session_ref,
        raw: value.clone(),
    })
}

async fn chat_error_from_response(response: reqwest::Response, path: &str) -> ChatError {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    chat_error_from_status_text(status, path, &text)
}

pub(super) fn chat_error_from_status_text(status: StatusCode, path: &str, text: &str) -> ChatError {
    let payload = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
    let error_code = string_field(&payload, "error").unwrap_or("");
    let payload_message = string_field(&payload, "message").unwrap_or("");
    let message = if payload_message.is_empty() {
        format!("{path} returned {status}")
    } else {
        payload_message.to_string()
    };
    match status {
        StatusCode::UNAUTHORIZED => ChatError::AuthRequired(format!("{path} requires daemon auth")),
        StatusCode::NOT_FOUND => ChatError::NotFound(message),
        StatusCode::CONFLICT => ChatError::Conflict {
            kind: conflict_kind(error_code, &message),
            message,
        },
        StatusCode::NOT_IMPLEMENTED => ChatError::StreamUnsupported(message),
        StatusCode::BAD_GATEWAY if error_code == "server_proxy_failed" => {
            ChatError::ServerProxyFailed(message)
        }
        StatusCode::BAD_GATEWAY if looks_like_stream_mapping(error_code, &message) => {
            ChatError::StreamUnsupported(message)
        }
        status if status.is_server_error() => ChatError::Server(message),
        _ => ChatError::Http {
            status: status.as_u16(),
            message,
        },
    }
}

fn conflict_kind(error_code: &str, message: &str) -> ChatConflictKind {
    match error_code {
        "session_busy" => ChatConflictKind::SessionBusy,
        "server_not_running" => ChatConflictKind::ServerStopped,
        "ambiguous_server" => ChatConflictKind::MultipleRunningServers,
        "no_running_server" => ChatConflictKind::NoRunningServer,
        "session_compaction_required" | "compaction_required" => {
            ChatConflictKind::CompactionRequired
        }
        "session_compaction_failed" | "compaction_failed" => ChatConflictKind::CompactionFailed,
        _ if message.contains("compaction") && message.contains("required") => {
            ChatConflictKind::CompactionRequired
        }
        _ if message.contains("compaction") && message.contains("failed") => {
            ChatConflictKind::CompactionFailed
        }
        _ => ChatConflictKind::Other,
    }
}

fn looks_like_stream_mapping(error_code: &str, message: &str) -> bool {
    error_code.contains("stream")
        || error_code.contains("mapping")
        || message.contains("Server-Sent Events")
        || message.contains("stream")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChatStreamEvent {
    Delta(String),
    Done(String),
    Error(String),
}

#[derive(Debug, Default)]
pub(super) struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatStreamEvent>, ChatError> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some((index, boundary_len)) = find_event_boundary(&self.buffer) {
            let event_bytes: Vec<u8> = self.buffer.drain(..index).collect();
            self.buffer.drain(..boundary_len);
            if let Some(event) = parse_sse_block(&event_bytes)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    pub(super) fn finish(&self, saw_done: bool) -> Result<(), ChatError> {
        if saw_done {
            return Ok(());
        }
        if self
            .buffer
            .iter()
            .any(|byte| !matches!(*byte, b'\r' | b'\n' | b' ' | b'\t'))
        {
            return Err(ChatError::Protocol(
                "chat stream ended with an incomplete SSE frame".to_string(),
            ));
        }
        Err(ChatError::Protocol(
            "chat stream ended before a done event".to_string(),
        ))
    }
}

fn find_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn parse_sse_block(bytes: &[u8]) -> Result<Option<ChatStreamEvent>, ChatError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| ChatError::Protocol(format!("stream was not valid UTF-8: {error}")))?;
    let mut event = String::new();
    let mut data_lines = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start().to_string());
        }
    }
    if event.is_empty() || data_lines.is_empty() {
        return Ok(None);
    }
    let data = data_lines.join("\n");
    match event.as_str() {
        "delta" => {
            let payload = serde_json::from_str::<Value>(&data).map_err(|error| {
                ChatError::Protocol(format!("invalid delta event JSON: {error}"))
            })?;
            let Some(delta) = payload.get("delta").and_then(Value::as_str) else {
                return Err(ChatError::Protocol(
                    "delta event missing string `delta`".to_string(),
                ));
            };
            Ok(Some(ChatStreamEvent::Delta(delta.to_string())))
        }
        "done" => {
            let payload = serde_json::from_str::<Value>(&data).map_err(|error| {
                ChatError::Protocol(format!("invalid done event JSON: {error}"))
            })?;
            let finish_reason = payload
                .get("finish_reason")
                .and_then(Value::as_str)
                .unwrap_or("stop");
            Ok(Some(ChatStreamEvent::Done(finish_reason.to_string())))
        }
        "error" => {
            let payload = serde_json::from_str::<Value>(&data).unwrap_or(Value::String(data));
            let message = string_field(&payload, "message")
                .or_else(|| string_field(&payload, "error"))
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| payload.to_string());
            Ok(Some(ChatStreamEvent::Error(message)))
        }
        _ => Ok(None),
    }
}

fn first_nonempty<'a>(values: &[Option<&'a str>]) -> Option<&'a str> {
    values
        .iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
        .copied()
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn clamp_to_len(index: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        index.min(len - 1)
    }
}

fn move_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let max = len - 1;
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs()).min(max)
    } else {
        current.saturating_add(delta as usize).min(max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_route_allowlist_stays_narrow_and_skips_auth() {
        let all_routes = CHAT_ALLOWED_GET_ROUTES
            .into_iter()
            .chain(CHAT_ALLOWED_POST_ROUTES)
            .collect::<Vec<_>>();

        assert!(!all_routes.iter().any(|route| route.contains("/v1/auth")));
        assert!(!all_routes.iter().any(|route| route.contains("PATCH")));
        assert_eq!(CHAT_ALLOWED_POST_ROUTES, ["/v1/sessions", "/v1/chat"]);
    }

    #[test]
    fn chat_paths_percent_encode_session_ref() {
        assert_eq!(
            chat_session_path("provider:abc/def ghi"),
            "/v1/sessions/provider%3Aabc%2Fdef%20ghi"
        );
        assert_eq!(
            chat_messages_path("ses/ref"),
            "/v1/sessions/ses%2Fref/messages?tail=50"
        );
    }

    #[test]
    fn new_session_request_uses_selected_server_and_no_patch_fields() {
        let body = session_create_body(
            "TUI chat now".to_string(),
            "server-1".to_string(),
            Some("adapter-1".to_string()),
        );

        assert_eq!(body["title"], "TUI chat now");
        assert_eq!(body["default_server_ref"], "server-1");
        assert_eq!(body["adapter_ref"], "adapter-1");
        assert!(body["messages"].as_array().expect("messages").is_empty());
        assert!(body.get("patch").is_none());
    }

    #[test]
    fn chat_send_body_matches_existing_daemon_dto() {
        let request = ChatSendRequest {
            request_id: 42,
            server_ref: "server".to_string(),
            session_ref: "session".to_string(),
            adapter_ref: Some("adapter".to_string()),
            prompt: "hello".to_string(),
            context_mode: ChatContextMode::Last10,
            max_session_messages: ChatContextMode::Last10.max_session_messages(),
            stream: true,
        };
        let body = request.body();

        assert_eq!(request.request_id, 42);
        assert_eq!(request.context_mode, ChatContextMode::Last10);
        assert_eq!(body["server_ref"], "server");
        assert_eq!(body["session_ref"], "session");
        assert_eq!(body["max_session_messages"], 10);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["max_tokens"], 512);
        assert_eq!(body["temperature"], 0.0);
        assert_eq!(body["stream"], true);
        assert_eq!(body["adapter_ref"], "adapter");

        let retry = request.clone().with_request_id(77).non_stream();
        assert_eq!(retry.request_id, 77);
        assert_eq!(retry.server_ref, request.server_ref);
        assert_eq!(retry.session_ref, request.session_ref);
        assert_eq!(retry.adapter_ref, request.adapter_ref);
        assert_eq!(retry.prompt, request.prompt);
        assert_eq!(retry.context_mode, request.context_mode);
        assert_eq!(retry.max_session_messages, request.max_session_messages);
        assert!(!retry.stream);
    }

    #[test]
    fn chat_context_mode_defaults_and_cycles() {
        let mut state = ChatState::default();

        assert_eq!(state.context_mode, ChatContextMode::Last2);
        assert_eq!(state.context_mode.max_session_messages(), 2);

        state.context_mode = ChatContextMode::None;
        state.cycle_context_mode();
        assert_eq!(state.context_mode, ChatContextMode::Last2);
        state.cycle_context_mode();
        assert_eq!(state.context_mode, ChatContextMode::Last10);
        state.cycle_context_mode();
        assert_eq!(state.context_mode, ChatContextMode::Last50);
        state.cycle_context_mode();
        assert_eq!(state.context_mode, ChatContextMode::None);
    }

    #[test]
    fn chat_context_modes_map_to_max_session_messages() {
        assert_eq!(ChatContextMode::None.max_session_messages(), 0);
        assert_eq!(ChatContextMode::Last2.max_session_messages(), 2);
        assert_eq!(ChatContextMode::Last10.max_session_messages(), 10);
        assert_eq!(ChatContextMode::Last50.max_session_messages(), 50);
    }

    #[test]
    fn chat_context_warnings_are_local_and_bounded_to_transcript() {
        let mut state = ChatState {
            context_mode: ChatContextMode::Last50,
            total_messages: Some(20),
            transcript: vec![
                ChatMessageRow {
                    index: Some(0),
                    role: "assistant".to_string(),
                    content: "hello there".to_string(),
                    created_at: None,
                    server_ref: None,
                    adapter_ref: None,
                },
                ChatMessageRow {
                    index: Some(1),
                    role: "assistant".to_string(),
                    content: "Hello again".to_string(),
                    created_at: None,
                    server_ref: None,
                    adapter_ref: None,
                },
            ],
            ..ChatState::default()
        };

        assert!(state.long_context_warning());
        assert!(state.greeting_loop_warning());

        state.context_mode = ChatContextMode::Last2;
        assert!(!state.long_context_warning());
    }

    #[test]
    fn sse_parser_handles_delta_done_comments_and_unknown_events() {
        let mut decoder = SseDecoder::default();
        let events = decoder
            .push(
                b": keepalive\n\n\
event: unknown\ndata: {\"x\":1}\n\n\
event: delta\ndata: {\"delta\":\"hi\"}\n\n\
event: done\ndata: {\"finish_reason\":\"stop\"}\n\n",
            )
            .expect("events");

        assert_eq!(
            events,
            vec![
                ChatStreamEvent::Delta("hi".to_string()),
                ChatStreamEvent::Done("stop".to_string())
            ]
        );
        assert!(decoder.finish(true).is_ok());
    }

    #[test]
    fn sse_parser_handles_error_event() {
        let mut decoder = SseDecoder::default();
        let events = decoder
            .push(b"event: error\ndata: {\"message\":\"bad\"}\n\n")
            .expect("events");

        assert_eq!(events, vec![ChatStreamEvent::Error("bad".to_string())]);
    }

    #[test]
    fn sse_parser_rejects_malformed_json_and_eof_before_done() {
        let mut decoder = SseDecoder::default();
        assert!(matches!(
            decoder.push(b"event: delta\ndata: nope\n\n"),
            Err(ChatError::Protocol(_))
        ));

        let mut decoder = SseDecoder::default();
        decoder
            .push(b"event: delta\ndata: {\"delta\":\"hi\"}\n\n")
            .expect("delta");
        assert!(matches!(
            decoder.finish(false),
            Err(ChatError::Protocol(message)) if message.contains("before a done")
        ));
    }

    #[test]
    fn status_mapping_covers_chat_errors() {
        let auth = chat_error_from_status_text(StatusCode::UNAUTHORIZED, "/v1/chat", "{}");
        assert!(auth.is_auth_required());

        let busy = chat_error_from_status_text(
            StatusCode::CONFLICT,
            "/v1/chat",
            r#"{"error":"session_busy","message":"busy"}"#,
        );
        assert!(matches!(
            busy,
            ChatError::Conflict {
                kind: ChatConflictKind::SessionBusy,
                ..
            }
        ));

        let stopped = chat_error_from_status_text(
            StatusCode::CONFLICT,
            "/v1/chat",
            r#"{"error":"server_not_running","message":"stopped"}"#,
        );
        assert!(matches!(
            stopped,
            ChatError::Conflict {
                kind: ChatConflictKind::ServerStopped,
                ..
            }
        ));

        let unsupported = chat_error_from_status_text(
            StatusCode::NOT_IMPLEMENTED,
            "/v1/chat",
            r#"{"error":"stream_not_implemented","message":"no stream"}"#,
        );
        assert!(unsupported.is_stream_unsupported());

        let proxy = chat_error_from_status_text(
            StatusCode::BAD_GATEWAY,
            "/v1/chat",
            r#"{"error":"server_proxy_failed","message":"target down"}"#,
        );
        assert!(matches!(proxy, ChatError::ServerProxyFailed(_)));
    }

    #[test]
    fn parse_servers_keeps_only_running_servers() {
        let rows = parse_servers(json!({
            "servers": [
                {"server_ref":"srv1","short_ref":"srv1","runtime_kind":"cloud","running":false},
                {"server_ref":"srv2","short_ref":"srv2","runtime_kind":"cloud","running":true,"port":1234}
            ]
        }))
        .expect("servers");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].server_ref, "srv2");
    }

    #[test]
    fn chat_state_prevents_double_submit_by_send_state() {
        let mut state = ChatState::default();
        assert!(state.send_state.is_idle());

        state.start_pending_send(7, "hello".to_string());

        assert!(!state.send_state.is_idle());
        assert!(state.send_state.is_in_flight());
        assert_eq!(state.pending_user.as_deref(), Some("hello"));
    }
}
