use std::cell::RefCell;
use std::path::PathBuf;

use crate::foundation::error::KernelResult;

use super::domain::{
    SessionAppendOutcome, SessionAppendedMessage, SessionChatContextMessage,
    SessionCompactionOutcome, SessionCompactionSummary, SessionFilePaths, SessionInspection,
    SessionMessage, SessionMessageRole, SessionMessages, SessionMetadata, SessionRef,
    SessionRefSelector, SessionRemovalOutcome, SessionRequestContextSummaryInput,
    SessionSqlStoreConfig, SessionStorageLocation, SessionStoreConfig, SessionStoreLayout,
    SessionSummary, StoredSessionMessage, SESSION_REF_HEX_LENGTH,
};
use super::ports::{
    SessionAdapterRefResolutionRequest, SessionAdapterRefResolver, SessionAppendMutation,
    SessionClock, SessionCreateRecord, SessionIdentityGenerator, SessionLock, SessionLockGuard,
    SessionLockManager, SessionPortFuture, SessionServerRefResolutionRequest,
    SessionServerRefResolver, SessionStore, SessionSummaryGenerationRequest,
    SessionSummaryGenerator, SessionSummaryInput, SessionTranscriptRewrite,
};

#[test]
fn session_store_config_keeps_file_layout_and_sql_config_separate() {
    let file = SessionStoreConfig::file_from_home_dir("/tmp/tentgent-home");
    let sql = SessionStoreConfig::Sql(SessionSqlStoreConfig {
        backend: "sqlite".to_string(),
        database_url: "sqlite:///tmp/tentgent/session.db".to_string(),
        runtime_home_dir: Some(PathBuf::from("/tmp/tentgent-home")),
    });

    let file_layout = file.file_layout().expect("file layout");
    assert_eq!(
        file_layout.sessions_dir,
        PathBuf::from("/tmp/tentgent-home/sessions")
    );
    assert_eq!(
        file.runtime_home_dir(),
        Some(PathBuf::from("/tmp/tentgent-home").as_path())
    );

    assert!(sql.file_layout().is_none());
    assert_eq!(
        sql.runtime_home_dir(),
        Some(PathBuf::from("/tmp/tentgent-home").as_path())
    );
}

#[test]
fn session_storage_location_represents_file_and_external_backends() {
    let session_ref = session_ref();
    let layout = SessionStoreLayout::from_home_dir("/tmp/tentgent-home");
    let file_location = layout.file_location(&session_ref);

    assert_eq!(
        file_location,
        SessionStorageLocation::File(SessionFilePaths {
            store_path: PathBuf::from("/tmp/tentgent-home/sessions").join(session_ref.as_str()),
            metadata_path: PathBuf::from("/tmp/tentgent-home/sessions")
                .join(session_ref.as_str())
                .join("session.toml"),
            messages_path: PathBuf::from("/tmp/tentgent-home/sessions")
                .join(session_ref.as_str())
                .join("messages.jsonl"),
        })
    );
    assert_eq!(
        SessionStorageLocation::External {
            backend: "sqlite".to_string(),
            locator: Some(format!("session:{}", session_ref.as_str())),
        },
        external_location(&session_ref)
    );
}

#[test]
fn session_ports_cover_backend_neutral_store_and_summary_boundaries() {
    let session_ref = session_ref();
    let sql = SessionStoreConfig::Sql(SessionSqlStoreConfig {
        backend: "sqlite".to_string(),
        database_url: "sqlite:///tmp/tentgent/session.db".to_string(),
        runtime_home_dir: Some(PathBuf::from("/tmp/tentgent-home")),
    });
    let store = FakeSessionStore::new(session_ref.clone());
    let identity = FakeSessionIdentityGenerator {
        session_ref: session_ref.clone(),
    };
    let locks = FakeSessionLockManager;
    let server_refs = FakeSessionServerRefResolver;
    let adapter_refs = FakeSessionAdapterRefResolver;
    let summaries = FakeSessionSummaryGenerator;

    assert_eq!(
        identity.generate_session_ref(&sql).expect("session ref"),
        session_ref
    );
    assert_eq!(FakeSessionClock.now_rfc3339().expect("clock"), CREATED_AT);
    drop(locks.acquire_create_lock(&sql).expect("create lock"));
    drop(
        locks
            .acquire_session_lock(&sql, &session_ref)
            .expect("session lock"),
    );

    store.ensure_session_store(&sql).expect("ensure store");
    let metadata = session_metadata(&session_ref, 1);
    let create = store
        .create_session(
            &sql,
            &session_ref,
            SessionCreateRecord {
                metadata: metadata.clone(),
                initial_messages: vec![stored_message(SessionMessageRole::User, "你好")],
            },
        )
        .expect("create");
    assert_eq!(create.location, external_location(&session_ref));

    assert_eq!(store.list_sessions(&sql).expect("list").len(), 1);
    assert_eq!(
        store
            .inspect_session(
                &sql,
                &SessionRefSelector::parse(session_ref.as_str()).expect("selector")
            )
            .expect("inspect")
            .location,
        external_location(&session_ref)
    );
    assert_eq!(
        store
            .load_session_metadata(&sql, &session_ref)
            .expect("metadata")
            .message_count,
        1
    );
    assert_eq!(
        store
            .read_tail_messages(&sql, &session_ref, 10)
            .expect("tail")
            .total_messages,
        1
    );

    let append = store
        .append_session_messages(
            &sql,
            &session_ref,
            SessionAppendMutation {
                metadata: session_metadata(&session_ref, 2),
                messages: vec![stored_message(SessionMessageRole::Assistant, "您好")],
                appended: vec![SessionAppendedMessage {
                    index: 1,
                    role: SessionMessageRole::Assistant,
                    created_at: CREATED_AT.to_string(),
                }],
            },
        )
        .expect("append");
    assert_eq!(append.appended[0].index, 1);

    let compaction = store
        .rewrite_session_transcript(
            &sql,
            &session_ref,
            SessionTranscriptRewrite {
                metadata: session_metadata(&session_ref, 1),
                replacement: vec![stored_message(SessionMessageRole::System, "摘要")],
                compacted: true,
                source_message_count: 2,
                replaced_message_count: 2,
                kept_recent_messages: 0,
                summary_index: Some(0),
            },
        )
        .expect("rewrite");
    assert!(compaction.compacted);

    assert_eq!(
        server_refs
            .resolve_session_server_ref(SessionServerRefResolutionRequest {
                store: sql.clone(),
                selector: "server123".to_string(),
            })
            .expect("server ref"),
        "server:server123"
    );
    assert_eq!(
        adapter_refs
            .resolve_session_adapter_ref(SessionAdapterRefResolutionRequest {
                store: sql.clone(),
                selector: "adapter123".to_string(),
            })
            .expect("adapter ref"),
        "adapter:adapter123"
    );

    let summary_future = summaries.summarize_session(SessionSummaryGenerationRequest {
        input: SessionSummaryInput::RequestContext(SessionRequestContextSummaryInput {
            prompt_messages: vec![SessionChatContextMessage {
                role: SessionMessageRole::User,
                content: "請摘要".to_string(),
            }],
            source_message_count: 3,
            summarized_message_count: 2,
            kept_recent_messages: 1,
        }),
        default_server_ref: Some("server:server123".to_string()),
        adapter_ref: None,
    });
    drop(summary_future);

    assert_eq!(
        store
            .remove_session(&sql, &session_ref)
            .expect("remove")
            .inspection
            .location,
        external_location(&session_ref)
    );
}

const CREATED_AT: &str = "2026-05-17T00:00:00Z";

#[derive(Debug)]
struct NoopSessionLock;

impl SessionLockGuard for NoopSessionLock {}

#[derive(Debug, Clone)]
struct FakeSessionIdentityGenerator {
    session_ref: SessionRef,
}

impl SessionIdentityGenerator for FakeSessionIdentityGenerator {
    fn generate_session_ref(&self, _store: &SessionStoreConfig) -> KernelResult<SessionRef> {
        Ok(self.session_ref.clone())
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeSessionClock;

impl SessionClock for FakeSessionClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok(CREATED_AT.to_string())
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeSessionLockManager;

impl SessionLockManager for FakeSessionLockManager {
    fn acquire_create_lock(&self, _store: &SessionStoreConfig) -> KernelResult<SessionLock> {
        Ok(Box::new(NoopSessionLock))
    }

    fn acquire_session_lock(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
    ) -> KernelResult<SessionLock> {
        Ok(Box::new(NoopSessionLock))
    }
}

#[derive(Debug, Clone)]
struct FakeSessionStore {
    session_ref: SessionRef,
    metadata: RefCell<Option<SessionMetadata>>,
    messages: RefCell<Vec<SessionMessage>>,
}

impl FakeSessionStore {
    fn new(session_ref: SessionRef) -> Self {
        Self {
            session_ref,
            metadata: RefCell::new(None),
            messages: RefCell::new(Vec::new()),
        }
    }
}

impl SessionStore for FakeSessionStore {
    fn ensure_session_store(&self, _store: &SessionStoreConfig) -> KernelResult<()> {
        Ok(())
    }

    fn list_sessions(&self, _store: &SessionStoreConfig) -> KernelResult<Vec<SessionSummary>> {
        let Some(metadata) = self.metadata.borrow().clone() else {
            return Ok(Vec::new());
        };
        Ok(vec![SessionSummary {
            location: external_location(&self.session_ref),
            metadata,
        }])
    }

    fn inspect_session(
        &self,
        _store: &SessionStoreConfig,
        _selector: &SessionRefSelector,
    ) -> KernelResult<SessionInspection> {
        Ok(SessionInspection {
            metadata: self.metadata.borrow().clone().expect("metadata"),
            location: external_location(&self.session_ref),
            warnings: Vec::new(),
        })
    }

    fn load_session_metadata(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
    ) -> KernelResult<SessionMetadata> {
        Ok(self.metadata.borrow().clone().expect("metadata"))
    }

    fn read_all_messages(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
    ) -> KernelResult<Vec<SessionMessage>> {
        Ok(self.messages.borrow().clone())
    }

    fn read_tail_messages(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
        tail: usize,
    ) -> KernelResult<SessionMessages> {
        let messages = self.messages.borrow().clone();
        let total_messages = messages.len();
        let truncated = total_messages > tail;
        let messages = if truncated {
            messages[total_messages - tail..].to_vec()
        } else {
            messages
        };
        Ok(SessionMessages {
            session_ref: self.session_ref.clone(),
            short_ref: self.session_ref.short_ref().to_string(),
            messages,
            tail,
            total_messages,
            truncated,
            warnings: Vec::new(),
        })
    }

    fn create_session(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
        record: SessionCreateRecord,
    ) -> KernelResult<SessionInspection> {
        self.metadata.replace(Some(record.metadata));
        self.messages
            .replace(stored_to_session_messages(&record.initial_messages));
        self.inspect_session(
            &SessionStoreConfig::file_from_home_dir("/unused"),
            &SessionRefSelector::parse(self.session_ref.as_str()).expect("selector"),
        )
    }

    fn update_session_metadata(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
        metadata: SessionMetadata,
    ) -> KernelResult<SessionInspection> {
        self.metadata.replace(Some(metadata));
        self.inspect_session(
            &SessionStoreConfig::file_from_home_dir("/unused"),
            &SessionRefSelector::parse(self.session_ref.as_str()).expect("selector"),
        )
    }

    fn append_session_messages(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
        mutation: SessionAppendMutation,
    ) -> KernelResult<SessionAppendOutcome> {
        self.metadata.replace(Some(mutation.metadata.clone()));
        self.messages
            .borrow_mut()
            .extend(stored_to_session_messages(&mutation.messages));
        Ok(SessionAppendOutcome {
            metadata: mutation.metadata,
            location: external_location(&self.session_ref),
            appended: mutation.appended,
        })
    }

    fn rewrite_session_transcript(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
        rewrite: SessionTranscriptRewrite,
    ) -> KernelResult<SessionCompactionOutcome> {
        self.metadata.replace(Some(rewrite.metadata.clone()));
        self.messages
            .replace(stored_to_session_messages(&rewrite.replacement));
        Ok(SessionCompactionOutcome {
            metadata: rewrite.metadata,
            location: external_location(&self.session_ref),
            compacted: rewrite.compacted,
            source_message_count: rewrite.source_message_count,
            replaced_message_count: rewrite.replaced_message_count,
            kept_recent_messages: rewrite.kept_recent_messages,
            summary_index: rewrite.summary_index,
        })
    }

    fn remove_session(
        &self,
        _store: &SessionStoreConfig,
        _session_ref: &SessionRef,
    ) -> KernelResult<SessionRemovalOutcome> {
        let inspection = self.inspect_session(
            &SessionStoreConfig::file_from_home_dir("/unused"),
            &SessionRefSelector::parse(self.session_ref.as_str()).expect("selector"),
        )?;
        self.metadata.replace(None);
        self.messages.borrow_mut().clear();
        Ok(SessionRemovalOutcome { inspection })
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeSessionServerRefResolver;

impl SessionServerRefResolver for FakeSessionServerRefResolver {
    fn resolve_session_server_ref(
        &self,
        request: SessionServerRefResolutionRequest,
    ) -> KernelResult<String> {
        Ok(format!("server:{}", request.selector))
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeSessionAdapterRefResolver;

impl SessionAdapterRefResolver for FakeSessionAdapterRefResolver {
    fn resolve_session_adapter_ref(
        &self,
        request: SessionAdapterRefResolutionRequest,
    ) -> KernelResult<String> {
        Ok(format!("adapter:{}", request.selector))
    }
}

#[derive(Debug, Clone, Copy)]
struct FakeSessionSummaryGenerator;

impl SessionSummaryGenerator for FakeSessionSummaryGenerator {
    fn summarize_session(
        &self,
        request: SessionSummaryGenerationRequest,
    ) -> SessionPortFuture<'_, SessionCompactionSummary> {
        Box::pin(async move {
            Ok(SessionCompactionSummary {
                content: format!(
                    "summary for {} messages",
                    request.input.prompt_messages().len()
                ),
                server_ref: request.default_server_ref,
                model_ref: None,
                provider_model: None,
                adapter_ref: request.adapter_ref,
            })
        })
    }
}

fn session_ref() -> SessionRef {
    SessionRef::parse("a".repeat(SESSION_REF_HEX_LENGTH)).expect("session ref")
}

fn session_metadata(session_ref: &SessionRef, message_count: usize) -> SessionMetadata {
    let mut metadata = SessionMetadata::new(session_ref.clone(), CREATED_AT, CREATED_AT);
    metadata.message_count = message_count;
    metadata
}

fn stored_message(role: SessionMessageRole, content: &str) -> StoredSessionMessage {
    StoredSessionMessage::new(role, content, CREATED_AT)
}

fn stored_to_session_messages(messages: &[StoredSessionMessage]) -> Vec<SessionMessage> {
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| SessionMessage {
            index,
            role: message.role,
            content: message.content.clone(),
            created_at: message.created_at.clone(),
            server_ref: message.server_ref.clone(),
            adapter_ref: message.adapter_ref.clone(),
            metadata: message.metadata.clone(),
        })
        .collect()
}

fn external_location(session_ref: &SessionRef) -> SessionStorageLocation {
    SessionStorageLocation::External {
        backend: "sqlite".to_string(),
        locator: Some(format!("session:{}", session_ref.as_str())),
    }
}
