use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::features::session::domain::{
    SessionCompactionSummary, SessionCreateRequest, SessionMessageInput, SessionMessageRole,
    SessionOptionalStringPatch, SessionRef, SessionRefSelector, SessionStoreConfig,
    SessionUpdateRequest, SESSION_REF_HEX_LENGTH,
};
use crate::features::session::infra::{FileSessionLockManager, FileSessionStore};
use crate::features::session::ports::{
    SessionAdapterRefResolutionRequest, SessionAdapterRefResolver, SessionClock,
    SessionIdentityGenerator, SessionPortFuture, SessionServerRefResolutionRequest,
    SessionServerRefResolver, SessionSummaryGenerationRequest, SessionSummaryGenerator,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver};

use super::port as session_usecases;
use super::{
    SessionCatalogReadUseCase as _, SessionChatContextUseCase as _, SessionCompactionUseCase as _,
    SessionMutationUseCase as _, SessionSummaryUseCase as _,
};

#[tokio::test]
async fn standard_session_usecase_runs_file_store_workflows() {
    let root = temp_home("standard-file");
    let session_ref = SessionRef::parse("b".repeat(SESSION_REF_HEX_LENGTH)).expect("session ref");
    let layout_resolver = StdRuntimeLayoutResolver;
    let identity = FixedSessionIdentity {
        session_ref: session_ref.clone(),
    };
    let clock = IncrementingSessionClock::default();
    let locks = FileSessionLockManager::default();
    let store = FileSessionStore;
    let refs = PrefixSessionRefResolver;
    let summaries = FakeSessionSummaryGenerator;
    let usecase = super::StdSessionUseCase::new(
        &layout_resolver,
        &identity,
        &clock,
        &locks,
        &store,
        &refs,
        &refs,
        &summaries,
    );
    let selection = session_usecases::SessionStoreSelection::default_file(layout_input_for_home(
        &root,
        LayoutResolveMode::Create,
    ));

    let created = usecase
        .create_session(session_usecases::CreateSessionRequest {
            store: selection.clone(),
            create: SessionCreateRequest {
                title: Some(" Support ".to_string()),
                default_server_ref: Some(" server-a ".to_string()),
                adapter_ref: Some(" adapter-a ".to_string()),
                tags: vec![" zh-tw ".to_string()],
                messages: vec![message_input(SessionMessageRole::User, "你好")],
            },
        })
        .expect("create");
    assert_eq!(created.inspection.metadata.session_ref, session_ref);
    assert_eq!(
        created.inspection.metadata.title.as_deref(),
        Some("Support")
    );
    assert_eq!(
        created.inspection.metadata.default_server_ref.as_deref(),
        Some("server:server-a")
    );
    assert_eq!(
        created.store.layout.as_ref().expect("layout").sessions_dir,
        root.join("sessions")
    );

    let listed = usecase
        .list_sessions(session_usecases::SessionListRequest {
            store: selection.clone(),
        })
        .expect("list");
    assert_eq!(listed.sessions.len(), 1);

    let selector = selector(&session_ref);
    let updated = usecase
        .update_session(session_usecases::UpdateSessionRequest {
            store: selection.clone(),
            selector: selector.clone(),
            update: SessionUpdateRequest {
                title: SessionOptionalStringPatch::Set("Updated".to_string()),
                default_server_ref: SessionOptionalStringPatch::Set("server-b".to_string()),
                adapter_ref: SessionOptionalStringPatch::Clear,
                tags: Some(vec!["support".to_string(), "verified".to_string()]),
            },
        })
        .expect("update");
    assert_eq!(
        updated.inspection.metadata.title.as_deref(),
        Some("Updated")
    );
    assert_eq!(
        updated.inspection.metadata.default_server_ref.as_deref(),
        Some("server:server-b")
    );
    assert!(updated.inspection.metadata.adapter_ref.is_none());

    let append = usecase
        .append_session_messages(session_usecases::AppendSessionMessagesRequest {
            store: selection.clone(),
            selector: selector.clone(),
            messages: vec![
                message_input(SessionMessageRole::Assistant, "您好"),
                message_input(SessionMessageRole::User, "請繼續"),
            ],
        })
        .expect("append");
    let session_usecases::AppendSessionMessagesResult::Appended {
        outcome,
        clear_compaction,
        ..
    } = append
    else {
        panic!("append should not require compaction");
    };
    assert!(clear_compaction.is_none());
    assert_eq!(outcome.metadata.message_count, 3);
    assert_eq!(outcome.appended.len(), 2);

    let prepared = usecase
        .prepare_session_compaction(session_usecases::PrepareSessionCompactionRequest {
            store: selection.clone(),
            selector: selector.clone(),
            keep_recent_messages: 1,
            instructions: Some("keep support facts".to_string()),
        })
        .expect("prepare compaction");
    let requirement = match prepared {
        session_usecases::PrepareSessionCompactionResult::SummaryRequired {
            requirement, ..
        } => requirement,
        session_usecases::PrepareSessionCompactionResult::NoOp { .. } => {
            panic!("three messages with one recent should need compaction")
        }
    };
    assert_eq!(
        requirement.default_server_ref.as_deref(),
        Some("server:server-b")
    );
    let generated = usecase
        .summarize_session_requirement(session_usecases::SessionSummaryUseCaseRequest {
            requirement: requirement.clone(),
        })
        .await
        .expect("summary");
    assert!(generated.summary.content.contains("summary for"));

    let compacted = usecase
        .apply_session_compaction(session_usecases::ApplySessionCompactionRequest {
            store: selection.clone(),
            selector: selector.clone(),
            keep_recent_messages: 1,
            instructions: None,
            summary: summary("manual summary"),
        })
        .expect("apply compaction");
    assert!(compacted.outcome.compacted);
    assert_eq!(compacted.outcome.metadata.message_count, 2);

    let messages = usecase
        .read_session_messages(session_usecases::SessionMessagesRequest {
            store: selection.clone(),
            selector: selector.clone(),
            tail: 10,
        })
        .expect("messages");
    assert_eq!(messages.messages.total_messages, 2);
    assert_eq!(
        messages.messages.messages[0].role,
        SessionMessageRole::System
    );
    assert_eq!(
        messages.messages.messages[0]
            .metadata
            .get("kind")
            .and_then(serde_json::Value::as_str),
        Some("session_summary")
    );

    let removed = usecase
        .remove_session(session_usecases::RemoveSessionRequest {
            store: selection,
            selector,
        })
        .expect("remove");
    assert_eq!(removed.outcome.inspection.metadata.session_ref, session_ref);
    assert!(!root.join("sessions").join(session_ref.as_str()).exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn standard_session_usecase_prepares_chat_context_and_applies_summaries() {
    let root = temp_home("standard-chat");
    let session_ref = SessionRef::parse("c".repeat(SESSION_REF_HEX_LENGTH)).expect("session ref");
    let layout_resolver = StdRuntimeLayoutResolver;
    let identity = FixedSessionIdentity {
        session_ref: session_ref.clone(),
    };
    let clock = IncrementingSessionClock::default();
    let locks = FileSessionLockManager::default();
    let store = FileSessionStore;
    let refs = PrefixSessionRefResolver;
    let summaries = FakeSessionSummaryGenerator;
    let usecase = super::StdSessionUseCase::new(
        &layout_resolver,
        &identity,
        &clock,
        &locks,
        &store,
        &refs,
        &refs,
        &summaries,
    );
    let selection = session_usecases::SessionStoreSelection::default_file(layout_input_for_home(
        &root,
        LayoutResolveMode::Create,
    ));
    let initial_messages = (0..22)
        .map(|index| message_input(SessionMessageRole::User, &format!("history {index}")))
        .collect::<Vec<_>>();
    let created = usecase
        .create_session(session_usecases::CreateSessionRequest {
            store: selection.clone(),
            create: SessionCreateRequest {
                title: Some("Chat".to_string()),
                default_server_ref: Some("server-a".to_string()),
                adapter_ref: None,
                tags: Vec::new(),
                messages: initial_messages,
            },
        })
        .expect("create");
    let selector = selector(&created.inspection.metadata.session_ref);

    let prepared = usecase
        .prepare_session_chat_turn(session_usecases::PrepareSessionChatTurnRequest {
            store: selection.clone(),
            selector: selector.clone(),
            max_session_messages: 5,
            request_messages: vec![message_input(SessionMessageRole::User, "下一步？")],
        })
        .expect("prepare chat");
    assert!(prepared.rolling_context.is_some());
    assert!(prepared.persisted_compaction.is_none());
    assert!(prepared.request_context_summary.is_some());
    assert_eq!(prepared.context_messages.len(), 1);

    let request_summary = usecase
        .apply_session_chat_summary(session_usecases::ApplySessionChatSummaryRequest {
            store: selection.clone(),
            selector: selector.clone(),
            max_session_messages: 5,
            request_messages: vec![message_input(SessionMessageRole::User, "下一步？")],
            scope: session_usecases::SessionChatSummaryScope::RequestContext,
            summary: summary("request summary"),
        })
        .expect("apply request summary");
    assert!(request_summary.request_context_summary_applied);
    assert_eq!(
        request_summary.turn.context_messages[0].role,
        SessionMessageRole::System
    );
    assert_eq!(request_summary.turn.context_messages.len(), 6);

    let rolling = usecase
        .apply_session_chat_summary(session_usecases::ApplySessionChatSummaryRequest {
            store: selection.clone(),
            selector: selector.clone(),
            max_session_messages: 5,
            request_messages: vec![message_input(SessionMessageRole::User, "下一步？")],
            scope: session_usecases::SessionChatSummaryScope::RollingContext,
            summary: summary("rolling summary"),
        })
        .expect("apply rolling summary");
    assert!(rolling.compaction.expect("rolling compaction").compacted);
    assert!(rolling.turn.metadata.message_count <= 11);

    let assistant = usecase
        .append_session_chat_assistant(session_usecases::AppendSessionChatAssistantRequest {
            store: selection,
            selector,
            request_messages: vec![message_input(SessionMessageRole::User, "下一步？")],
            assistant_content: "可以。".to_string(),
            assistant_server_ref: Some("server:server-a".to_string()),
            assistant_adapter_ref: None,
            assistant_metadata: serde_json::json!({"finish_reason": "stop"}),
        })
        .expect("append assistant");
    assert_eq!(assistant.outcome.appended.len(), 2);
    let _ = fs::remove_dir_all(root);
}

#[derive(Debug, Clone)]
struct FixedSessionIdentity {
    session_ref: SessionRef,
}

impl SessionIdentityGenerator for FixedSessionIdentity {
    fn generate_session_ref(&self, _store: &SessionStoreConfig) -> KernelResult<SessionRef> {
        Ok(self.session_ref.clone())
    }
}

#[derive(Debug, Default)]
struct IncrementingSessionClock {
    tick: Cell<usize>,
}

impl SessionClock for IncrementingSessionClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        let tick = self.tick.get();
        self.tick.set(tick + 1);
        Ok(format!("2026-05-17T00:{tick:02}:00Z"))
    }
}

#[derive(Debug, Clone, Copy)]
struct PrefixSessionRefResolver;

impl SessionServerRefResolver for PrefixSessionRefResolver {
    fn resolve_session_server_ref(
        &self,
        request: SessionServerRefResolutionRequest,
    ) -> KernelResult<String> {
        Ok(format!("server:{}", request.selector))
    }
}

impl SessionAdapterRefResolver for PrefixSessionRefResolver {
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
                model_ref: Some("summary-model".to_string()),
                provider_model: None,
                adapter_ref: request.adapter_ref,
            })
        })
    }
}

fn selector(session_ref: &SessionRef) -> SessionRefSelector {
    SessionRefSelector::parse(session_ref.as_str()).expect("selector")
}

fn layout_input_for_home(
    home_dir: impl Into<PathBuf>,
    mode: LayoutResolveMode,
) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: Some(home_dir.into()),
        data_root_dir: None,
    }
}

fn message_input(role: SessionMessageRole, content: &str) -> SessionMessageInput {
    SessionMessageInput {
        role,
        content: content.to_string(),
        server_ref: None,
        adapter_ref: None,
        metadata: serde_json::Value::Object(Default::default()),
    }
}

fn summary(content: &str) -> SessionCompactionSummary {
    SessionCompactionSummary {
        content: content.to_string(),
        server_ref: Some("server123".to_string()),
        model_ref: Some("model123".to_string()),
        provider_model: None,
        adapter_ref: None,
    }
}

fn temp_home(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-session-usecase-{name}-{}-{nanos}",
        std::process::id()
    ))
}
