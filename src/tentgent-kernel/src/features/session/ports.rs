//! Session feature package ports.

use std::{future::Future, pin::Pin};

use crate::foundation::error::KernelResult;

use super::domain::{
    SessionAppendOutcome, SessionAppendedMessage, SessionChatContextMessage,
    SessionCompactionInput, SessionCompactionOutcome, SessionCompactionSummary, SessionInspection,
    SessionMessage, SessionMessages, SessionMetadata, SessionRef, SessionRefSelector,
    SessionRemovalOutcome, SessionRequestContextSummaryInput, SessionStoreConfig, SessionSummary,
    StoredSessionMessage,
};

pub type SessionPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Opaque guard returned by a session lock manager.
pub trait SessionLockGuard: std::fmt::Debug {}

pub type SessionLock = Box<dyn SessionLockGuard>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionServerRefResolutionRequest {
    pub store: SessionStoreConfig,
    pub selector: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionAdapterRefResolutionRequest {
    pub store: SessionStoreConfig,
    pub selector: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSummaryInput {
    PersistedCompaction(SessionCompactionInput),
    RollingContext(SessionCompactionInput),
    RequestContext(SessionRequestContextSummaryInput),
}

impl SessionSummaryInput {
    pub fn prompt_messages(&self) -> &[SessionChatContextMessage] {
        match self {
            Self::PersistedCompaction(input) | Self::RollingContext(input) => {
                &input.prompt_messages
            }
            Self::RequestContext(input) => &input.prompt_messages,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummaryGenerationRequest {
    pub input: SessionSummaryInput,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionCreateRecord {
    pub metadata: SessionMetadata,
    pub initial_messages: Vec<StoredSessionMessage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionAppendMutation {
    pub metadata: SessionMetadata,
    pub messages: Vec<StoredSessionMessage>,
    pub appended: Vec<SessionAppendedMessage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionTranscriptRewrite {
    pub metadata: SessionMetadata,
    pub replacement: Vec<StoredSessionMessage>,
    pub compacted: bool,
    pub source_message_count: usize,
    pub replaced_message_count: usize,
    pub kept_recent_messages: usize,
    pub summary_index: Option<usize>,
}

/// Generates unique managed session refs.
pub trait SessionIdentityGenerator {
    /// Generates a full session ref that does not collide in the provided layout.
    fn generate_session_ref(&self, store: &SessionStoreConfig) -> KernelResult<SessionRef>;
}

/// Supplies timestamps for durable session records and transcript messages.
pub trait SessionClock {
    /// Returns the current UTC timestamp formatted as RFC3339.
    fn now_rfc3339(&self) -> KernelResult<String>;
}

/// Acquires session-store locks for create, update, append, compact, and remove workflows.
pub trait SessionLockManager {
    /// Acquires the store-wide create lock used while allocating a new session ref.
    fn acquire_create_lock(&self, store: &SessionStoreConfig) -> KernelResult<SessionLock>;

    /// Acquires the per-session lock used while mutating a resolved session.
    fn acquire_session_lock(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<SessionLock>;
}

/// Backend-neutral session store boundary.
///
/// Implementations may persist sessions in files, SQLite, or another local
/// store. Callers pass semantic records and mutations rather than file names.
pub trait SessionStore {
    /// Ensures the backing store is ready for mutating session operations.
    fn ensure_session_store(&self, store: &SessionStoreConfig) -> KernelResult<()>;

    /// Lists stored session summaries sorted for stable display.
    fn list_sessions(&self, store: &SessionStoreConfig) -> KernelResult<Vec<SessionSummary>>;

    /// Resolves a full session ref or unique prefix and returns backend location details.
    fn inspect_session(
        &self,
        store: &SessionStoreConfig,
        selector: &SessionRefSelector,
    ) -> KernelResult<SessionInspection>;

    /// Loads metadata for an already resolved session ref.
    fn load_session_metadata(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<SessionMetadata>;

    /// Reads all transcript messages for a resolved session ref.
    fn read_all_messages(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<Vec<SessionMessage>>;

    /// Reads the last `tail` transcript messages and total count for display.
    fn read_tail_messages(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        tail: usize,
    ) -> KernelResult<SessionMessages>;

    /// Creates a session with already validated metadata and initial transcript messages.
    fn create_session(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        record: SessionCreateRecord,
    ) -> KernelResult<SessionInspection>;

    /// Replaces stored metadata after a session update.
    fn update_session_metadata(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        metadata: SessionMetadata,
    ) -> KernelResult<SessionInspection>;

    /// Appends transcript messages and persists the resulting metadata in one store mutation.
    fn append_session_messages(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        mutation: SessionAppendMutation,
    ) -> KernelResult<SessionAppendOutcome>;

    /// Rewrites transcript content and persists compaction metadata in one store mutation.
    fn rewrite_session_transcript(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        rewrite: SessionTranscriptRewrite,
    ) -> KernelResult<SessionCompactionOutcome>;

    /// Removes a resolved session.
    fn remove_session(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
    ) -> KernelResult<SessionRemovalOutcome>;
}

/// Resolves user-provided server refs for session defaults and summary generation.
pub trait SessionServerRefResolver {
    /// Resolves a server ref or unique prefix into a full server ref string.
    fn resolve_session_server_ref(
        &self,
        request: SessionServerRefResolutionRequest,
    ) -> KernelResult<String>;
}

/// Resolves user-provided adapter refs for session defaults and summary generation.
pub trait SessionAdapterRefResolver {
    /// Resolves an adapter ref or unique prefix into a full adapter ref string.
    fn resolve_session_adapter_ref(
        &self,
        request: SessionAdapterRefResolutionRequest,
    ) -> KernelResult<String>;
}

/// Produces session summaries through chat/model infrastructure outside the session store.
pub trait SessionSummaryGenerator {
    /// Generates a summary from prepared session prompt messages.
    fn summarize_session(
        &self,
        request: SessionSummaryGenerationRequest,
    ) -> SessionPortFuture<'_, SessionCompactionSummary>;
}
