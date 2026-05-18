//! Session use case ports.

use std::{future::Future, pin::Pin};

use serde_json::Value;

use crate::features::session::domain::{
    SessionAppendOutcome, SessionChatContextMessage, SessionCompactionOutcome,
    SessionCompactionSummary, SessionCreateRequest as SessionCreateInput, SessionInspection,
    SessionMessageInput, SessionMessages, SessionMetadata, SessionRef, SessionRefSelector,
    SessionRemovalOutcome, SessionStoreConfig, SessionSummary,
    SessionUpdateRequest as SessionUpdateInput,
};
use crate::features::session::ports::SessionSummaryInput;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by session use cases that call model/chat infrastructure.
pub type SessionUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Session store selected by a caller before runtime-home resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStoreSelection {
    /// Use the default file-backed session store for the resolved runtime home.
    DefaultFile { layout: RuntimeLayoutInput },
    /// Use a fully specified backend-neutral store config.
    Explicit(SessionStoreConfig),
}

impl SessionStoreSelection {
    pub fn default_file(layout: RuntimeLayoutInput) -> Self {
        Self::DefaultFile { layout }
    }

    pub fn explicit(store: SessionStoreConfig) -> Self {
        Self::Explicit(store)
    }
}

/// Effective session store after resolving any caller-level default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSessionStore {
    pub layout: Option<RuntimeLayout>,
    pub store: SessionStoreConfig,
}

impl ResolvedSessionStore {
    pub fn file(layout: RuntimeLayout) -> Self {
        let store = SessionStoreConfig::file_from_sessions_dir(layout.sessions_dir.clone());
        Self {
            layout: Some(layout),
            store,
        }
    }

    pub fn explicit(store: SessionStoreConfig) -> Self {
        Self {
            layout: None,
            store,
        }
    }
}

/// Summary work that a session use case needs before continuing a mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummaryRequirement {
    pub input: SessionSummaryInput,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
}

/// Request for listing sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListRequest {
    pub store: SessionStoreSelection,
}

/// Result of listing sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListResult {
    pub store: ResolvedSessionStore,
    pub sessions: Vec<SessionSummary>,
}

/// Request for inspecting one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInspectRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
}

/// Result of inspecting one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInspectResult {
    pub store: ResolvedSessionStore,
    pub inspection: SessionInspection,
}

/// Request for reading recent session messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMessagesRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub tail: usize,
}

/// Result of reading recent session messages.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionMessagesResult {
    pub store: ResolvedSessionStore,
    pub messages: SessionMessages,
}

/// Request for creating a session.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateSessionRequest {
    pub store: SessionStoreSelection,
    pub create: SessionCreateInput,
}

/// Result of creating a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSessionResult {
    pub store: ResolvedSessionStore,
    pub inspection: SessionInspection,
}

/// Request for updating one session's metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateSessionRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub update: SessionUpdateInput,
}

/// Result of updating one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSessionResult {
    pub store: ResolvedSessionStore,
    pub inspection: SessionInspection,
}

/// Request for appending transcript messages without a caller-provided compaction summary.
#[derive(Debug, Clone, PartialEq)]
pub struct AppendSessionMessagesRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub messages: Vec<SessionMessageInput>,
}

/// Result of an append attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendSessionMessagesResult {
    Appended {
        store: ResolvedSessionStore,
        outcome: SessionAppendOutcome,
        clear_compaction: Option<SessionCompactionOutcome>,
    },
    CompactionRequired {
        store: ResolvedSessionStore,
        session_ref: SessionRef,
        requirement: SessionSummaryRequirement,
    },
}

/// Request for retrying an append after the caller has obtained a compaction summary.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplySessionAppendCompactionRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub messages: Vec<SessionMessageInput>,
    pub summary: SessionCompactionSummary,
}

/// Result of applying append compaction and appending messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySessionAppendCompactionResult {
    pub store: ResolvedSessionStore,
    pub compaction: Option<SessionCompactionOutcome>,
    pub outcome: SessionAppendOutcome,
}

/// Request for removing one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveSessionRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
}

/// Result of removing one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveSessionResult {
    pub store: ResolvedSessionStore,
    pub outcome: SessionRemovalOutcome,
}

/// Request for preparing a manual session compaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareSessionCompactionRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub keep_recent_messages: usize,
    pub instructions: Option<String>,
}

/// Result of preparing a manual session compaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareSessionCompactionResult {
    NoOp {
        store: ResolvedSessionStore,
        outcome: SessionCompactionOutcome,
    },
    SummaryRequired {
        store: ResolvedSessionStore,
        session_ref: SessionRef,
        requirement: SessionSummaryRequirement,
    },
}

/// Request for applying a manual session compaction summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySessionCompactionRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub keep_recent_messages: usize,
    pub instructions: Option<String>,
    pub summary: SessionCompactionSummary,
}

/// Result of applying a manual session compaction summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySessionCompactionResult {
    pub store: ResolvedSessionStore,
    pub outcome: SessionCompactionOutcome,
}

/// Session chat summary scope selected while preparing a chat turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionChatSummaryScope {
    RollingContext,
    PersistedCompaction,
    RequestContext,
}

/// Request for preparing the session context for one chat turn.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepareSessionChatTurnRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub max_session_messages: usize,
    pub request_messages: Vec<SessionMessageInput>,
}

/// Prepared session context and any summary work needed before generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareSessionChatTurnResult {
    pub store: ResolvedSessionStore,
    pub metadata: SessionMetadata,
    pub context_messages: Vec<SessionChatContextMessage>,
    pub max_session_messages: usize,
    pub historical_messages: usize,
    pub truncated: bool,
    pub clear_compaction: Option<SessionCompactionOutcome>,
    pub rolling_context: Option<SessionSummaryRequirement>,
    pub persisted_compaction: Option<SessionSummaryRequirement>,
    pub request_context_summary: Option<SessionSummaryRequirement>,
}

/// Request for applying one summary required by a prepared chat turn.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplySessionChatSummaryRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub max_session_messages: usize,
    pub request_messages: Vec<SessionMessageInput>,
    pub scope: SessionChatSummaryScope,
    pub summary: SessionCompactionSummary,
}

/// Result of applying one chat-turn summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySessionChatSummaryResult {
    pub turn: PrepareSessionChatTurnResult,
    pub compaction: Option<SessionCompactionOutcome>,
    pub request_context_summary_applied: bool,
}

/// Request for appending request messages and the assistant response after generation.
#[derive(Debug, Clone, PartialEq)]
pub struct AppendSessionChatAssistantRequest {
    pub store: SessionStoreSelection,
    pub selector: SessionRefSelector,
    pub request_messages: Vec<SessionMessageInput>,
    pub assistant_content: String,
    pub assistant_server_ref: Option<String>,
    pub assistant_adapter_ref: Option<String>,
    pub assistant_metadata: Value,
}

/// Result of appending request messages and the assistant response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendSessionChatAssistantResult {
    pub store: ResolvedSessionStore,
    pub outcome: SessionAppendOutcome,
}

/// Request for generating a summary required by a session use case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummaryUseCaseRequest {
    pub requirement: SessionSummaryRequirement,
}

/// Result of generating a summary required by a session use case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummaryUseCaseResult {
    pub summary: SessionCompactionSummary,
}

/// Use-case boundary for resolving caller-level session store selection.
pub trait SessionStoreResolutionUseCase {
    /// Resolves a default file store from runtime layout or passes through an explicit store.
    fn resolve_session_store(
        &self,
        selection: SessionStoreSelection,
    ) -> KernelResult<ResolvedSessionStore>;
}

/// Use-case boundary for read-only session catalog operations.
pub trait SessionCatalogReadUseCase {
    /// Lists sessions without mutating the store.
    fn list_sessions(&self, request: SessionListRequest) -> KernelResult<SessionListResult>;

    /// Inspects one session by full session ref or unique prefix.
    fn inspect_session(&self, request: SessionInspectRequest)
        -> KernelResult<SessionInspectResult>;

    /// Reads the most recent transcript messages for one session.
    fn read_session_messages(
        &self,
        request: SessionMessagesRequest,
    ) -> KernelResult<SessionMessagesResult>;
}

/// Use-case boundary for session create, update, append, and remove operations.
pub trait SessionMutationUseCase {
    /// Creates a session and records optional initial messages.
    fn create_session(&self, request: CreateSessionRequest) -> KernelResult<CreateSessionResult>;

    /// Updates one session's metadata.
    fn update_session(&self, request: UpdateSessionRequest) -> KernelResult<UpdateSessionResult>;

    /// Appends messages or returns the summary work required before append can continue.
    fn append_session_messages(
        &self,
        request: AppendSessionMessagesRequest,
    ) -> KernelResult<AppendSessionMessagesResult>;

    /// Applies a caller-provided compaction summary and appends the pending messages.
    fn apply_session_append_compaction(
        &self,
        request: ApplySessionAppendCompactionRequest,
    ) -> KernelResult<ApplySessionAppendCompactionResult>;

    /// Removes one resolved session.
    fn remove_session(&self, request: RemoveSessionRequest) -> KernelResult<RemoveSessionResult>;
}

/// Use-case boundary for manual session compaction.
pub trait SessionCompactionUseCase {
    /// Prepares manual compaction or returns a no-op outcome when nothing needs rewriting.
    fn prepare_session_compaction(
        &self,
        request: PrepareSessionCompactionRequest,
    ) -> KernelResult<PrepareSessionCompactionResult>;

    /// Applies a caller-provided manual compaction summary.
    fn apply_session_compaction(
        &self,
        request: ApplySessionCompactionRequest,
    ) -> KernelResult<ApplySessionCompactionResult>;
}

/// Use-case boundary for preparing and persisting session-backed chat turns.
pub trait SessionChatContextUseCase {
    /// Prepares context messages and reports any summary work needed before chat generation.
    fn prepare_session_chat_turn(
        &self,
        request: PrepareSessionChatTurnRequest,
    ) -> KernelResult<PrepareSessionChatTurnResult>;

    /// Applies one chat-turn summary and returns the updated prepared context.
    fn apply_session_chat_summary(
        &self,
        request: ApplySessionChatSummaryRequest,
    ) -> KernelResult<ApplySessionChatSummaryResult>;

    /// Appends the user request messages plus the assistant response after generation.
    fn append_session_chat_assistant(
        &self,
        request: AppendSessionChatAssistantRequest,
    ) -> KernelResult<AppendSessionChatAssistantResult>;
}

/// Use-case boundary for generating session summaries through chat/model infrastructure.
pub trait SessionSummaryUseCase {
    /// Generates a summary for a requirement returned by another session use case.
    fn summarize_session_requirement(
        &self,
        request: SessionSummaryUseCaseRequest,
    ) -> SessionUseCaseFuture<'_, SessionSummaryUseCaseResult>;
}
