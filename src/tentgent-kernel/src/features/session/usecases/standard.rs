//! Standard session use case orchestration.

use crate::features::session::domain::{
    SessionCompactionOutcome, SessionMessage, SessionMessageInput, SessionMessageRole,
    SessionMetadata, SessionRef, SessionRefSelector, SessionStoreConfig,
    MAX_SESSION_CONTEXT_MESSAGES, SESSION_SCHEMA,
};
use crate::features::session::ports::{
    SessionAdapterRefResolutionRequest, SessionAdapterRefResolver, SessionClock,
    SessionCreateRecord, SessionIdentityGenerator, SessionLock, SessionLockManager,
    SessionServerRefResolutionRequest, SessionServerRefResolver, SessionStore,
    SessionSummaryGenerationRequest, SessionSummaryGenerator, SessionSummaryInput,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    append_mutation, apply_optional_string_patch, bounded_compaction_action,
    build_chat_context_messages, build_context_from_request_plan, build_stored_messages,
    compacted_replacement_messages, compaction_input_from_plan, no_op_compaction_outcome,
    normalize_optional_string, normalize_required_string, normalize_tags,
    request_context_historical_messages, request_context_plan, request_context_truncated,
    rolling_compaction_action, rolling_context_input_from_plan,
    rolling_context_replacement_messages, session_usecase_error, summary_requirement_defaults,
    transcript_rewrite, validate_chat_message_inputs, validate_compact_request,
    validate_message_inputs, validate_metadata_object, validate_protected_count,
    BoundedCompactionAction, RollingCompactionAction,
};
use super::port::{
    AppendSessionChatAssistantRequest, AppendSessionChatAssistantResult,
    AppendSessionMessagesRequest, AppendSessionMessagesResult, ApplySessionAppendCompactionRequest,
    ApplySessionAppendCompactionResult, ApplySessionChatSummaryRequest,
    ApplySessionChatSummaryResult, ApplySessionCompactionRequest, ApplySessionCompactionResult,
    CreateSessionRequest, CreateSessionResult, PrepareSessionChatTurnRequest,
    PrepareSessionChatTurnResult, PrepareSessionCompactionRequest, PrepareSessionCompactionResult,
    RemoveSessionRequest, RemoveSessionResult, ResolvedSessionStore, SessionCatalogReadUseCase,
    SessionChatContextUseCase, SessionChatSummaryScope, SessionCompactionUseCase,
    SessionInspectRequest, SessionInspectResult, SessionListRequest, SessionListResult,
    SessionMessagesRequest, SessionMessagesResult, SessionMutationUseCase,
    SessionStoreResolutionUseCase, SessionStoreSelection, SessionSummaryRequirement,
    SessionSummaryUseCase, SessionSummaryUseCaseRequest, SessionSummaryUseCaseResult,
    SessionUseCaseFuture, UpdateSessionRequest, UpdateSessionResult,
};

/// Standard session use case implementation built from session ports.
pub struct StdSessionUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    identity: &'a dyn SessionIdentityGenerator,
    clock: &'a dyn SessionClock,
    locks: &'a dyn SessionLockManager,
    store: &'a dyn SessionStore,
    server_refs: &'a dyn SessionServerRefResolver,
    adapter_refs: &'a dyn SessionAdapterRefResolver,
    summaries: &'a dyn SessionSummaryGenerator,
}

impl<'a> StdSessionUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        identity: &'a dyn SessionIdentityGenerator,
        clock: &'a dyn SessionClock,
        locks: &'a dyn SessionLockManager,
        store: &'a dyn SessionStore,
        server_refs: &'a dyn SessionServerRefResolver,
        adapter_refs: &'a dyn SessionAdapterRefResolver,
        summaries: &'a dyn SessionSummaryGenerator,
    ) -> Self {
        Self {
            layout_resolver,
            identity,
            clock,
            locks,
            store,
            server_refs,
            adapter_refs,
            summaries,
        }
    }

    fn resolve_selection(
        &self,
        selection: SessionStoreSelection,
    ) -> KernelResult<ResolvedSessionStore> {
        match selection {
            SessionStoreSelection::DefaultFile { layout } => {
                let layout = self.layout_resolver.resolve(layout)?;
                Ok(ResolvedSessionStore::file(layout))
            }
            SessionStoreSelection::Explicit(store) => Ok(ResolvedSessionStore::explicit(store)),
        }
    }

    fn resolve_optional_server_ref(
        &self,
        store: &SessionStoreConfig,
        selector: Option<String>,
    ) -> KernelResult<Option<String>> {
        let Some(selector) = normalize_optional_string(selector, "default_server_ref")? else {
            return Ok(None);
        };
        self.server_refs
            .resolve_session_server_ref(SessionServerRefResolutionRequest {
                store: store.clone(),
                selector,
            })
            .map(Some)
    }

    fn resolve_optional_adapter_ref(
        &self,
        store: &SessionStoreConfig,
        selector: Option<String>,
    ) -> KernelResult<Option<String>> {
        let Some(selector) = normalize_optional_string(selector, "adapter_ref")? else {
            return Ok(None);
        };
        self.adapter_refs
            .resolve_session_adapter_ref(SessionAdapterRefResolutionRequest {
                store: store.clone(),
                selector,
            })
            .map(Some)
    }

    fn resolve_server_ref(
        &self,
        store: &SessionStoreConfig,
        selector: String,
    ) -> KernelResult<String> {
        let selector = normalize_required_string(selector, "default_server_ref")?;
        self.server_refs
            .resolve_session_server_ref(SessionServerRefResolutionRequest {
                store: store.clone(),
                selector,
            })
    }

    fn resolve_adapter_ref(
        &self,
        store: &SessionStoreConfig,
        selector: String,
    ) -> KernelResult<String> {
        let selector = normalize_required_string(selector, "adapter_ref")?;
        self.adapter_refs
            .resolve_session_adapter_ref(SessionAdapterRefResolutionRequest {
                store: store.clone(),
                selector,
            })
    }

    fn inspect_lock_and_load(
        &self,
        store: &SessionStoreConfig,
        selector: &SessionRefSelector,
    ) -> KernelResult<LockedSession> {
        let inspection = self.store.inspect_session(store, selector)?;
        let session_ref = inspection.metadata.session_ref.clone();
        let lock = self.locks.acquire_session_lock(store, &session_ref)?;
        let metadata = self.store.load_session_metadata(store, &session_ref)?;
        let messages = self.store.read_all_messages(store, &session_ref)?;

        Ok(LockedSession {
            session_ref,
            location: inspection.location,
            metadata,
            messages,
            _lock: lock,
        })
    }

    fn append_messages_after_optional_clear(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        metadata: SessionMetadata,
        existing_messages: Vec<SessionMessage>,
        messages: Vec<SessionMessageInput>,
        clear_existing: bool,
    ) -> KernelResult<(
        Option<SessionCompactionOutcome>,
        crate::features::session::domain::SessionAppendOutcome,
    )> {
        let (metadata, current_count, clear_compaction) = if clear_existing {
            let updated_at = self.clock.now_rfc3339()?;
            let rewrite = transcript_rewrite(
                metadata,
                Vec::new(),
                updated_at,
                existing_messages.len(),
                existing_messages.len(),
                0,
                None,
            );
            let outcome = self
                .store
                .rewrite_session_transcript(store, session_ref, rewrite)?;
            (outcome.metadata.clone(), 0, Some(outcome))
        } else {
            (metadata, existing_messages.len(), None)
        };

        let created_at = self.clock.now_rfc3339()?;
        let stored_messages = build_stored_messages(messages, &created_at);
        let mutation = append_mutation(metadata, current_count, stored_messages, created_at);
        let outcome = self
            .store
            .append_session_messages(store, session_ref, mutation)?;
        Ok((clear_compaction, outcome))
    }

    fn apply_persisted_summary(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        metadata: SessionMetadata,
        plan: super::common::CompactionPlan,
        summary: crate::features::session::domain::SessionCompactionSummary,
    ) -> KernelResult<SessionCompactionOutcome> {
        let compacted_at = self.clock.now_rfc3339()?;
        let (replacement, summary_index) =
            compacted_replacement_messages(&plan, summary, compacted_at.clone())?;
        let rewrite = transcript_rewrite(
            metadata,
            replacement,
            compacted_at,
            plan.source_messages.len() + plan.recent_messages.len(),
            plan.source_messages.len(),
            plan.recent_messages.len(),
            Some(summary_index),
        );
        self.store
            .rewrite_session_transcript(store, session_ref, rewrite)
    }

    fn apply_rolling_summary(
        &self,
        store: &SessionStoreConfig,
        session_ref: &SessionRef,
        metadata: SessionMetadata,
        plan: super::common::CompactionPlan,
        summary: crate::features::session::domain::SessionCompactionSummary,
    ) -> KernelResult<SessionCompactionOutcome> {
        let compacted_at = self.clock.now_rfc3339()?;
        let (replacement, summary_index) =
            rolling_context_replacement_messages(&plan, summary, compacted_at.clone())?;
        let rewrite = transcript_rewrite(
            metadata,
            replacement,
            compacted_at,
            plan.source_messages.len() + plan.recent_messages.len(),
            plan.source_messages.len(),
            plan.recent_messages.len(),
            Some(summary_index),
        );
        self.store
            .rewrite_session_transcript(store, session_ref, rewrite)
    }

    fn summary_requirement(
        &self,
        metadata: &SessionMetadata,
        input: SessionSummaryInput,
    ) -> SessionSummaryRequirement {
        let (default_server_ref, adapter_ref) = summary_requirement_defaults(metadata);
        SessionSummaryRequirement {
            input,
            default_server_ref,
            adapter_ref,
        }
    }

    fn prepare_chat_turn_result(
        &self,
        selection: SessionStoreSelection,
        selector: SessionRefSelector,
        max_session_messages: usize,
        request_messages: Vec<SessionMessageInput>,
        request_summary_content: Option<&str>,
    ) -> KernelResult<(PrepareSessionChatTurnResult, bool)> {
        if max_session_messages > MAX_SESSION_CONTEXT_MESSAGES {
            return Err(session_usecase_error(format!(
                "`max_session_messages` must be at most {MAX_SESSION_CONTEXT_MESSAGES}"
            )));
        }
        let request_messages = validate_chat_message_inputs(request_messages)?;
        let protected_count = request_messages.len() + 1;
        validate_protected_count(protected_count)?;

        let resolved = self.resolve_selection(selection)?;
        let mut locked = self.inspect_lock_and_load(&resolved.store, &selector)?;

        let mut clear_compaction = None;
        if matches!(
            bounded_compaction_action(&locked.messages, protected_count)?,
            BoundedCompactionAction::Clear
        ) {
            let updated_at = self.clock.now_rfc3339()?;
            let rewrite = transcript_rewrite(
                locked.metadata,
                Vec::new(),
                updated_at,
                locked.messages.len(),
                locked.messages.len(),
                0,
                None,
            );
            let outcome = self.store.rewrite_session_transcript(
                &resolved.store,
                &locked.session_ref,
                rewrite,
            )?;
            locked.metadata = outcome.metadata.clone();
            locked.messages.clear();
            clear_compaction = Some(outcome);
        }

        let rolling_context = match rolling_compaction_action(&locked.messages, protected_count)? {
            RollingCompactionAction::None => None,
            RollingCompactionAction::Summarize(plan) => Some(self.summary_requirement(
                &locked.metadata,
                SessionSummaryInput::RollingContext(rolling_context_input_from_plan(&plan)?),
            )),
        };

        let persisted_compaction =
            match bounded_compaction_action(&locked.messages, protected_count)? {
                BoundedCompactionAction::Summarize(plan) => Some(self.summary_requirement(
                    &locked.metadata,
                    SessionSummaryInput::PersistedCompaction(compaction_input_from_plan(
                        &plan, None,
                    )?),
                )),
                BoundedCompactionAction::None | BoundedCompactionAction::Clear => None,
            };

        let (context_messages, historical_messages, truncated, request_context_summary, applied) =
            if persisted_compaction.is_some() {
                (
                    build_chat_context_messages(&[], &request_messages)?,
                    0,
                    !locked.messages.is_empty(),
                    None,
                    false,
                )
            } else {
                let plan = request_context_plan(
                    &locked.messages,
                    max_session_messages,
                    &request_messages,
                )?;
                let request_context_summary = match &plan {
                    super::common::RequestContextPlan::SummaryPlusRecent {
                        summary_input, ..
                    } => Some(self.summary_requirement(
                        &locked.metadata,
                        SessionSummaryInput::RequestContext(summary_input.clone()),
                    )),
                    super::common::RequestContextPlan::NoHistory
                    | super::common::RequestContextPlan::RawHistory(_) => None,
                };
                let applied = request_summary_content.is_some()
                    && matches!(
                        plan,
                        super::common::RequestContextPlan::SummaryPlusRecent { .. }
                    );
                let context_messages = build_context_from_request_plan(
                    &plan,
                    request_summary_content,
                    &request_messages,
                )?;
                (
                    context_messages,
                    request_context_historical_messages(&plan),
                    request_context_truncated(&plan, locked.messages.len()),
                    request_context_summary,
                    applied,
                )
            };

        Ok((
            PrepareSessionChatTurnResult {
                store: resolved,
                metadata: locked.metadata,
                context_messages,
                max_session_messages,
                historical_messages,
                truncated,
                clear_compaction,
                rolling_context,
                persisted_compaction,
                request_context_summary,
            },
            applied,
        ))
    }
}

impl SessionStoreResolutionUseCase for StdSessionUseCase<'_> {
    fn resolve_session_store(
        &self,
        selection: SessionStoreSelection,
    ) -> KernelResult<ResolvedSessionStore> {
        self.resolve_selection(selection)
    }
}

impl SessionCatalogReadUseCase for StdSessionUseCase<'_> {
    fn list_sessions(&self, request: SessionListRequest) -> KernelResult<SessionListResult> {
        let resolved = self.resolve_selection(request.store)?;
        let sessions = self.store.list_sessions(&resolved.store)?;
        Ok(SessionListResult {
            store: resolved,
            sessions,
        })
    }

    fn inspect_session(
        &self,
        request: SessionInspectRequest,
    ) -> KernelResult<SessionInspectResult> {
        let resolved = self.resolve_selection(request.store)?;
        let inspection = self
            .store
            .inspect_session(&resolved.store, &request.selector)?;
        Ok(SessionInspectResult {
            store: resolved,
            inspection,
        })
    }

    fn read_session_messages(
        &self,
        request: SessionMessagesRequest,
    ) -> KernelResult<SessionMessagesResult> {
        let resolved = self.resolve_selection(request.store)?;
        let inspection = self
            .store
            .inspect_session(&resolved.store, &request.selector)?;
        let messages = self.store.read_tail_messages(
            &resolved.store,
            &inspection.metadata.session_ref,
            request.tail,
        )?;
        Ok(SessionMessagesResult {
            store: resolved,
            messages,
        })
    }
}

impl SessionMutationUseCase for StdSessionUseCase<'_> {
    fn create_session(&self, request: CreateSessionRequest) -> KernelResult<CreateSessionResult> {
        let resolved = self.resolve_selection(request.store)?;
        self.store.ensure_session_store(&resolved.store)?;

        let title = normalize_optional_string(request.create.title, "title")?;
        let tags = normalize_tags(request.create.tags)?;
        let default_server_ref =
            self.resolve_optional_server_ref(&resolved.store, request.create.default_server_ref)?;
        let adapter_ref =
            self.resolve_optional_adapter_ref(&resolved.store, request.create.adapter_ref)?;
        let messages = validate_message_inputs(request.create.messages, true)?;
        validate_protected_count(messages.len())?;

        let _lock = self.locks.acquire_create_lock(&resolved.store)?;
        let session_ref = self.identity.generate_session_ref(&resolved.store)?;
        let now = self.clock.now_rfc3339()?;
        let stored_messages = build_stored_messages(messages, &now);
        let mut metadata = SessionMetadata::new(session_ref.clone(), now.clone(), now);
        metadata.schema = SESSION_SCHEMA.to_string();
        metadata.title = title;
        metadata.message_count = stored_messages.len();
        metadata.default_server_ref = default_server_ref;
        metadata.adapter_ref = adapter_ref;
        metadata.tags = tags;

        let inspection = self.store.create_session(
            &resolved.store,
            &session_ref,
            SessionCreateRecord {
                metadata,
                initial_messages: stored_messages,
            },
        )?;

        Ok(CreateSessionResult {
            store: resolved,
            inspection,
        })
    }

    fn update_session(&self, request: UpdateSessionRequest) -> KernelResult<UpdateSessionResult> {
        if request.update.is_empty() {
            return Err(session_usecase_error(
                "session update must include at least one field",
            ));
        }

        let resolved = self.resolve_selection(request.store)?;
        let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
        let mut metadata = locked.metadata;

        metadata.title =
            apply_optional_string_patch(metadata.title, request.update.title, "title")?;
        metadata.default_server_ref = match request.update.default_server_ref {
            crate::features::session::domain::SessionOptionalStringPatch::Unchanged => {
                metadata.default_server_ref
            }
            crate::features::session::domain::SessionOptionalStringPatch::Clear => None,
            crate::features::session::domain::SessionOptionalStringPatch::Set(value) => {
                Some(self.resolve_server_ref(&resolved.store, value)?)
            }
        };
        metadata.adapter_ref = match request.update.adapter_ref {
            crate::features::session::domain::SessionOptionalStringPatch::Unchanged => {
                metadata.adapter_ref
            }
            crate::features::session::domain::SessionOptionalStringPatch::Clear => None,
            crate::features::session::domain::SessionOptionalStringPatch::Set(value) => {
                Some(self.resolve_adapter_ref(&resolved.store, value)?)
            }
        };
        if let Some(tags) = request.update.tags {
            metadata.tags = normalize_tags(tags)?;
        }
        metadata.updated_at = self.clock.now_rfc3339()?;

        let inspection =
            self.store
                .update_session_metadata(&resolved.store, &locked.session_ref, metadata)?;
        Ok(UpdateSessionResult {
            store: resolved,
            inspection,
        })
    }

    fn append_session_messages(
        &self,
        request: AppendSessionMessagesRequest,
    ) -> KernelResult<AppendSessionMessagesResult> {
        let messages = validate_message_inputs(request.messages, false)?;
        validate_protected_count(messages.len())?;

        let resolved = self.resolve_selection(request.store)?;
        let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
        match bounded_compaction_action(&locked.messages, messages.len())? {
            BoundedCompactionAction::None => {
                let (_, outcome) = self.append_messages_after_optional_clear(
                    &resolved.store,
                    &locked.session_ref,
                    locked.metadata,
                    locked.messages,
                    messages,
                    false,
                )?;
                Ok(AppendSessionMessagesResult::Appended {
                    store: resolved,
                    outcome,
                    clear_compaction: None,
                })
            }
            BoundedCompactionAction::Clear => {
                let (clear_compaction, outcome) = self.append_messages_after_optional_clear(
                    &resolved.store,
                    &locked.session_ref,
                    locked.metadata,
                    locked.messages,
                    messages,
                    true,
                )?;
                Ok(AppendSessionMessagesResult::Appended {
                    store: resolved,
                    outcome,
                    clear_compaction,
                })
            }
            BoundedCompactionAction::Summarize(plan) => {
                Ok(AppendSessionMessagesResult::CompactionRequired {
                    store: resolved,
                    session_ref: locked.session_ref,
                    requirement: self.summary_requirement(
                        &locked.metadata,
                        SessionSummaryInput::PersistedCompaction(compaction_input_from_plan(
                            &plan, None,
                        )?),
                    ),
                })
            }
        }
    }

    fn apply_session_append_compaction(
        &self,
        request: ApplySessionAppendCompactionRequest,
    ) -> KernelResult<ApplySessionAppendCompactionResult> {
        let messages = validate_message_inputs(request.messages, false)?;
        validate_protected_count(messages.len())?;

        let resolved = self.resolve_selection(request.store)?;
        let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
        let (metadata, source_messages, compaction) =
            match bounded_compaction_action(&locked.messages, messages.len())? {
                BoundedCompactionAction::None => (locked.metadata, locked.messages, None),
                BoundedCompactionAction::Clear => {
                    let updated_at = self.clock.now_rfc3339()?;
                    let rewrite = transcript_rewrite(
                        locked.metadata,
                        Vec::new(),
                        updated_at,
                        locked.messages.len(),
                        locked.messages.len(),
                        0,
                        None,
                    );
                    let outcome = self.store.rewrite_session_transcript(
                        &resolved.store,
                        &locked.session_ref,
                        rewrite,
                    )?;
                    (outcome.metadata.clone(), Vec::new(), Some(outcome))
                }
                BoundedCompactionAction::Summarize(plan) => {
                    let outcome = self.apply_persisted_summary(
                        &resolved.store,
                        &locked.session_ref,
                        locked.metadata,
                        plan,
                        request.summary,
                    )?;
                    let source_messages = self
                        .store
                        .read_all_messages(&resolved.store, &locked.session_ref)?;
                    (outcome.metadata.clone(), source_messages, Some(outcome))
                }
            };

        let (_, outcome) = self.append_messages_after_optional_clear(
            &resolved.store,
            &locked.session_ref,
            metadata,
            source_messages,
            messages,
            false,
        )?;
        Ok(ApplySessionAppendCompactionResult {
            store: resolved,
            compaction,
            outcome,
        })
    }

    fn remove_session(&self, request: RemoveSessionRequest) -> KernelResult<RemoveSessionResult> {
        let resolved = self.resolve_selection(request.store)?;
        let inspection = self
            .store
            .inspect_session(&resolved.store, &request.selector)?;
        let session_ref = inspection.metadata.session_ref.clone();
        let _lock = self
            .locks
            .acquire_session_lock(&resolved.store, &session_ref)?;
        let outcome = self.store.remove_session(&resolved.store, &session_ref)?;
        Ok(RemoveSessionResult {
            store: resolved,
            outcome,
        })
    }
}

impl SessionCompactionUseCase for StdSessionUseCase<'_> {
    fn prepare_session_compaction(
        &self,
        request: PrepareSessionCompactionRequest,
    ) -> KernelResult<PrepareSessionCompactionResult> {
        validate_compact_request(
            request.keep_recent_messages,
            request.instructions.as_deref(),
        )?;
        let resolved = self.resolve_selection(request.store)?;
        let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
        let Some(plan) =
            super::common::build_compaction_plan(&locked.messages, request.keep_recent_messages)
        else {
            return Ok(PrepareSessionCompactionResult::NoOp {
                store: resolved,
                outcome: no_op_compaction_outcome(
                    locked.metadata,
                    locked.location,
                    locked.messages.len(),
                ),
            });
        };
        let requirement = self.summary_requirement(
            &locked.metadata,
            SessionSummaryInput::PersistedCompaction(compaction_input_from_plan(
                &plan,
                request.instructions.as_deref(),
            )?),
        );
        Ok(PrepareSessionCompactionResult::SummaryRequired {
            store: resolved,
            session_ref: locked.session_ref,
            requirement,
        })
    }

    fn apply_session_compaction(
        &self,
        request: ApplySessionCompactionRequest,
    ) -> KernelResult<ApplySessionCompactionResult> {
        validate_compact_request(
            request.keep_recent_messages,
            request.instructions.as_deref(),
        )?;
        let resolved = self.resolve_selection(request.store)?;
        let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
        let outcome = match super::common::build_compaction_plan(
            &locked.messages,
            request.keep_recent_messages,
        ) {
            Some(plan) => self.apply_persisted_summary(
                &resolved.store,
                &locked.session_ref,
                locked.metadata,
                plan,
                request.summary,
            )?,
            None => {
                no_op_compaction_outcome(locked.metadata, locked.location, locked.messages.len())
            }
        };
        Ok(ApplySessionCompactionResult {
            store: resolved,
            outcome,
        })
    }
}

impl SessionChatContextUseCase for StdSessionUseCase<'_> {
    fn prepare_session_chat_turn(
        &self,
        request: PrepareSessionChatTurnRequest,
    ) -> KernelResult<PrepareSessionChatTurnResult> {
        self.prepare_chat_turn_result(
            request.store,
            request.selector,
            request.max_session_messages,
            request.request_messages,
            None,
        )
        .map(|(result, _)| result)
    }

    fn apply_session_chat_summary(
        &self,
        request: ApplySessionChatSummaryRequest,
    ) -> KernelResult<ApplySessionChatSummaryResult> {
        let selection = request.store.clone();
        let selector = request.selector.clone();
        let max_session_messages = request.max_session_messages;
        let request_messages = request.request_messages.clone();

        match request.scope {
            SessionChatSummaryScope::RollingContext => {
                let compaction = {
                    let resolved = self.resolve_selection(request.store)?;
                    let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
                    match rolling_compaction_action(&locked.messages, request_messages.len() + 1)? {
                        RollingCompactionAction::None => None,
                        RollingCompactionAction::Summarize(plan) => {
                            Some(self.apply_rolling_summary(
                                &resolved.store,
                                &locked.session_ref,
                                locked.metadata,
                                plan,
                                request.summary,
                            )?)
                        }
                    }
                };
                let (turn, _) = self.prepare_chat_turn_result(
                    selection,
                    selector,
                    max_session_messages,
                    request_messages,
                    None,
                )?;
                Ok(ApplySessionChatSummaryResult {
                    turn,
                    compaction,
                    request_context_summary_applied: false,
                })
            }
            SessionChatSummaryScope::PersistedCompaction => {
                let compaction = {
                    let resolved = self.resolve_selection(request.store)?;
                    let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
                    match bounded_compaction_action(&locked.messages, request_messages.len() + 1)? {
                        BoundedCompactionAction::None => None,
                        BoundedCompactionAction::Clear => {
                            let updated_at = self.clock.now_rfc3339()?;
                            let rewrite = transcript_rewrite(
                                locked.metadata,
                                Vec::new(),
                                updated_at,
                                locked.messages.len(),
                                locked.messages.len(),
                                0,
                                None,
                            );
                            Some(self.store.rewrite_session_transcript(
                                &resolved.store,
                                &locked.session_ref,
                                rewrite,
                            )?)
                        }
                        BoundedCompactionAction::Summarize(plan) => {
                            Some(self.apply_persisted_summary(
                                &resolved.store,
                                &locked.session_ref,
                                locked.metadata,
                                plan,
                                request.summary,
                            )?)
                        }
                    }
                };
                let (turn, _) = self.prepare_chat_turn_result(
                    selection,
                    selector,
                    max_session_messages,
                    request_messages,
                    None,
                )?;
                Ok(ApplySessionChatSummaryResult {
                    turn,
                    compaction,
                    request_context_summary_applied: false,
                })
            }
            SessionChatSummaryScope::RequestContext => {
                let summary_content = request.summary.content;
                let (turn, applied) = self.prepare_chat_turn_result(
                    request.store,
                    request.selector,
                    request.max_session_messages,
                    request.request_messages,
                    Some(&summary_content),
                )?;
                Ok(ApplySessionChatSummaryResult {
                    turn,
                    compaction: None,
                    request_context_summary_applied: applied,
                })
            }
        }
    }

    fn append_session_chat_assistant(
        &self,
        request: AppendSessionChatAssistantRequest,
    ) -> KernelResult<AppendSessionChatAssistantResult> {
        validate_metadata_object(&request.assistant_metadata, "assistant metadata")?;
        let mut messages = validate_chat_message_inputs(request.request_messages)?;
        let assistant = SessionMessageInput {
            role: SessionMessageRole::Assistant,
            content: request.assistant_content,
            server_ref: request.assistant_server_ref,
            adapter_ref: request.assistant_adapter_ref,
            metadata: request.assistant_metadata,
        };
        messages.push(assistant);
        let messages = validate_message_inputs(messages, false)?;
        validate_protected_count(messages.len())?;

        let resolved = self.resolve_selection(request.store)?;
        let locked = self.inspect_lock_and_load(&resolved.store, &request.selector)?;
        let (clear_existing, existing_messages) =
            match bounded_compaction_action(&locked.messages, messages.len())? {
                BoundedCompactionAction::None => (false, locked.messages),
                BoundedCompactionAction::Clear => (true, locked.messages),
                BoundedCompactionAction::Summarize(_) => {
                    return Err(session_usecase_error(
                        "session compaction is required before appending assistant response",
                    ));
                }
            };
        let (_, outcome) = self.append_messages_after_optional_clear(
            &resolved.store,
            &locked.session_ref,
            locked.metadata,
            existing_messages,
            messages,
            clear_existing,
        )?;
        Ok(AppendSessionChatAssistantResult {
            store: resolved,
            outcome,
        })
    }
}

impl SessionSummaryUseCase for StdSessionUseCase<'_> {
    fn summarize_session_requirement(
        &self,
        request: SessionSummaryUseCaseRequest,
    ) -> SessionUseCaseFuture<'_, SessionSummaryUseCaseResult> {
        Box::pin(async move {
            let summary = self
                .summaries
                .summarize_session(SessionSummaryGenerationRequest {
                    input: request.requirement.input,
                    default_server_ref: request.requirement.default_server_ref,
                    adapter_ref: request.requirement.adapter_ref,
                })
                .await?;
            Ok(SessionSummaryUseCaseResult { summary })
        })
    }
}

struct LockedSession {
    session_ref: SessionRef,
    location: crate::features::session::domain::SessionStorageLocation,
    metadata: SessionMetadata,
    messages: Vec<SessionMessage>,
    _lock: SessionLock,
}
