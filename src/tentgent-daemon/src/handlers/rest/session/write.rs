use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use tentgent_kernel::{
    features::{
        adapter::domain::AdapterRefSelector,
        auth::domain::Provider,
        chat::{
            domain::{ChatGenerationOptions, ChatMessage, ChatPrompt, ChatRole},
            usecases::{ChatCompletionUseCase, ChatPreparationRequest, ChatTargetSelection},
        },
        model::domain::ModelRefSelector,
        runtime::domain::PythonRuntimeResolutionInput,
        server::{
            domain::CloudProvider,
            usecases::{ServerInspectRequest, ServerSpecUseCase},
        },
        session::{
            domain::{
                SessionCompactionSummary, SessionCreateRequest as SessionCreateInput,
                SessionMessageInput, SessionMessageRole, SessionOptionalStringPatch,
                SessionUpdateRequest as SessionUpdateInput,
            },
            usecases::{
                AppendSessionMessagesRequest, AppendSessionMessagesResult,
                ApplySessionAppendCompactionRequest, ApplySessionCompactionRequest,
                CreateSessionRequest, PrepareSessionCompactionRequest,
                PrepareSessionCompactionResult, RemoveSessionRequest, SessionCompactionUseCase,
                SessionMutationUseCase, SessionSummaryRequirement, UpdateSessionRequest,
            },
        },
    },
    foundation::layout::LayoutResolveMode,
};

use crate::transport::rest::{error::RestError, state::RestState};

use super::{
    dto::{
        session_append_response, session_compact_response, session_inspection_item,
        session_remove_response, SessionAppendResponse, SessionCompactResponse,
        SessionCreateResponse, SessionRemoveResponse, SessionResponse,
    },
    parse_selector, session_error, session_mutation_error, session_store_selection,
};

const DEFAULT_COMPACT_KEEP_RECENT_MESSAGES: usize = 49;
const DEFAULT_SUMMARY_MAX_TOKENS: u32 = 512;
const DEFAULT_SUMMARY_TEMPERATURE: f32 = 0.2;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSessionBody {
    pub title: Option<String>,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub messages: Vec<SessionMessageBody>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSessionBody {
    #[serde(default)]
    pub title: Option<Option<String>>,
    #[serde(default)]
    pub default_server_ref: Option<Option<String>>,
    #[serde(default)]
    pub adapter_ref: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppendSessionMessagesBody {
    pub messages: Vec<SessionMessageBody>,
    pub compaction_server_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactSessionBody {
    pub server_ref: Option<String>,
    pub keep_recent_messages: Option<usize>,
    pub instructions: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMessageBody {
    pub role: String,
    pub content: String,
    pub server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    pub metadata: Option<Value>,
}

pub async fn create(
    State(state): State<RestState>,
    Json(body): Json<CreateSessionBody>,
) -> Result<(StatusCode, Json<SessionCreateResponse>), RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .create_session(CreateSessionRequest {
            store: session_store_selection(&state),
            create: SessionCreateInput {
                title: body.title,
                default_server_ref: body.default_server_ref,
                adapter_ref: body.adapter_ref,
                tags: body.tags,
                messages: parse_message_inputs(body.messages)?,
            },
        })
        .map_err(session_mutation_error)?;

    Ok((
        StatusCode::CREATED,
        Json(SessionCreateResponse {
            session: session_inspection_item(result.inspection),
            created: true,
        }),
    ))
}

pub async fn update(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(body): Json<UpdateSessionBody>,
) -> Result<Json<SessionResponse>, RestError> {
    let selector = parse_selector(&reference)?;
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .update_session(UpdateSessionRequest {
            store: session_store_selection(&state),
            selector,
            update: SessionUpdateInput {
                title: optional_string_patch(body.title),
                default_server_ref: optional_string_patch(body.default_server_ref),
                adapter_ref: optional_string_patch(body.adapter_ref),
                tags: body.tags,
            },
        })
        .map_err(session_mutation_error)?;

    Ok(Json(SessionResponse {
        session: session_inspection_item(result.inspection),
    }))
}

pub async fn append_messages(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(body): Json<AppendSessionMessagesBody>,
) -> Result<Json<SessionAppendResponse>, RestError> {
    let selector = parse_selector(&reference)?;
    let messages = parse_message_inputs(body.messages)?;
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .append_session_messages(AppendSessionMessagesRequest {
            store: session_store_selection(&state),
            selector: selector.clone(),
            messages: messages.clone(),
        })
        .map_err(session_mutation_error)?;

    match result {
        AppendSessionMessagesResult::Appended { outcome, .. } => {
            Ok(Json(session_append_response(outcome)))
        }
        AppendSessionMessagesResult::CompactionRequired { requirement, .. } => {
            let Some(server_ref) = body.compaction_server_ref else {
                return Err(RestError::conflict(
                    "session_compaction_required",
                    "session compaction is required before appending messages",
                ));
            };
            let summary = generate_summary(&state, Some(server_ref), requirement).await?;
            let result = state
                .app()
                .services()
                .kernel()
                .session_usecase()
                .apply_session_append_compaction(ApplySessionAppendCompactionRequest {
                    store: session_store_selection(&state),
                    selector,
                    messages,
                    summary,
                })
                .map_err(session_mutation_error)?;
            Ok(Json(session_append_response(result.outcome)))
        }
    }
}

pub async fn compact(
    State(state): State<RestState>,
    Path(reference): Path<String>,
    Json(body): Json<CompactSessionBody>,
) -> Result<Json<SessionCompactResponse>, RestError> {
    let selector = parse_selector(&reference)?;
    let keep_recent_messages = body
        .keep_recent_messages
        .unwrap_or(DEFAULT_COMPACT_KEEP_RECENT_MESSAGES);
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .prepare_session_compaction(PrepareSessionCompactionRequest {
            store: session_store_selection(&state),
            selector: selector.clone(),
            keep_recent_messages,
            instructions: body.instructions.clone(),
        })
        .map_err(session_mutation_error)?;

    match result {
        PrepareSessionCompactionResult::NoOp { outcome, .. } => {
            Ok(Json(session_compact_response(outcome)))
        }
        PrepareSessionCompactionResult::SummaryRequired { requirement, .. } => {
            let summary = generate_summary(&state, body.server_ref, requirement).await?;
            let result = state
                .app()
                .services()
                .kernel()
                .session_usecase()
                .apply_session_compaction(ApplySessionCompactionRequest {
                    store: session_store_selection(&state),
                    selector,
                    keep_recent_messages,
                    instructions: body.instructions,
                    summary,
                })
                .map_err(session_mutation_error)?;
            Ok(Json(session_compact_response(result.outcome)))
        }
    }
}

pub async fn remove(
    State(state): State<RestState>,
    Path(reference): Path<String>,
) -> Result<Json<SessionRemoveResponse>, RestError> {
    let selector = parse_selector(&reference)?;
    let result = state
        .app()
        .services()
        .kernel()
        .session_usecase()
        .remove_session(RemoveSessionRequest {
            store: session_store_selection(&state),
            selector,
        })
        .map_err(session_mutation_error)?;

    Ok(Json(session_remove_response(result.outcome)))
}

fn parse_message_inputs(
    messages: Vec<SessionMessageBody>,
) -> Result<Vec<SessionMessageInput>, RestError> {
    messages.into_iter().map(parse_message_input).collect()
}

fn parse_message_input(message: SessionMessageBody) -> Result<SessionMessageInput, RestError> {
    let role = SessionMessageRole::parse(&message.role)
        .map_err(|err| RestError::bad_request("bad_request", err.to_string()))?;
    let metadata = message
        .metadata
        .unwrap_or_else(|| Value::Object(Default::default()));
    if !metadata.is_object() {
        return Err(RestError::bad_request(
            "bad_request",
            "`metadata` must be a JSON object",
        ));
    }
    Ok(SessionMessageInput {
        role,
        content: message.content,
        server_ref: message.server_ref,
        adapter_ref: message.adapter_ref,
        metadata,
    })
}

fn optional_string_patch(value: Option<Option<String>>) -> SessionOptionalStringPatch {
    match value {
        None => SessionOptionalStringPatch::Unchanged,
        Some(None) => SessionOptionalStringPatch::Clear,
        Some(Some(value)) => SessionOptionalStringPatch::Set(value),
    }
}

async fn generate_summary(
    state: &RestState,
    server_ref: Option<String>,
    requirement: SessionSummaryRequirement,
) -> Result<SessionCompactionSummary, RestError> {
    let server_ref = server_ref
        .or(requirement.default_server_ref)
        .ok_or_else(|| {
            RestError::conflict(
                "session_compaction_required",
                "session compaction requires `server_ref` or session `default_server_ref`",
            )
        })?;
    let selector = tentgent_kernel::features::server::domain::ServerRefSelector::parse(&server_ref)
        .map_err(|err| {
            RestError::bad_request("bad_request", format!("invalid server_ref: {err}"))
        })?;
    let server = state
        .app()
        .services()
        .kernel()
        .server_usecase()
        .inspect_server(ServerInspectRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
            selector,
        })
        .map_err(session_error)?
        .inspection;
    let adapter_selector = requirement
        .adapter_ref
        .as_deref()
        .map(AdapterRefSelector::parse)
        .transpose()
        .map_err(|err| {
            RestError::bad_request("bad_request", format!("invalid adapter_ref: {err}"))
        })?;
    let target = match server.spec.runtime_kind {
        tentgent_kernel::features::server::domain::ServerRuntimeKind::Local => {
            let model_ref = server.spec.model_ref.as_ref().ok_or_else(|| {
                RestError::conflict(
                    "session_compaction_required",
                    format!("server `{}` is missing model_ref", server.spec.short_ref),
                )
            })?;
            ChatTargetSelection::LocalModel {
                model_selector: ModelRefSelector::parse(model_ref.as_str()).map_err(|err| {
                    RestError::bad_request("bad_request", format!("invalid model_ref: {err}"))
                })?,
                adapter_selector,
            }
        }
        tentgent_kernel::features::server::domain::ServerRuntimeKind::Cloud => {
            let provider = match server.spec.provider.ok_or_else(|| {
                RestError::conflict(
                    "session_compaction_required",
                    format!("server `{}` is missing provider", server.spec.short_ref),
                )
            })? {
                CloudProvider::OpenAI => Provider::OpenAI,
                CloudProvider::Anthropic => Provider::Anthropic,
                CloudProvider::Gemini => Provider::Gemini,
            };
            let provider_model = server.spec.provider_model.clone().ok_or_else(|| {
                RestError::conflict(
                    "session_compaction_required",
                    format!(
                        "server `{}` is missing provider_model",
                        server.spec.short_ref
                    ),
                )
            })?;
            ChatTargetSelection::CloudProvider {
                provider,
                provider_model,
            }
        }
    };
    let messages = requirement
        .input
        .prompt_messages()
        .iter()
        .map(|message| {
            ChatMessage::new(chat_role(message.role)?, message.content.clone())
                .map_err(|err| RestError::bad_request("bad_request", err.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let prompt = ChatPrompt::new(messages)
        .map_err(|err| RestError::bad_request("bad_request", err.to_string()))?;
    let request = ChatPreparationRequest {
        layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        runtime: PythonRuntimeResolutionInput::default(),
        target,
        prompt,
        options: ChatGenerationOptions {
            max_tokens: Some(DEFAULT_SUMMARY_MAX_TOKENS),
            temperature: Some(DEFAULT_SUMMARY_TEMPERATURE),
            stream: false,
        },
    };

    let task_state = state.clone();
    let handle = tokio::runtime::Handle::current();
    let result = tokio::task::spawn_blocking(move || {
        handle.block_on(async {
            task_state
                .app()
                .services()
                .kernel()
                .chat_usecase()
                .complete_chat(request)
                .await
        })
    })
    .await
    .map_err(|err| RestError::internal("session_compaction_failed", err.to_string()))?
    .map_err(|err| RestError::kernel("session_compaction_failed", err))?;

    Ok(SessionCompactionSummary {
        content: result.response.text,
        server_ref: Some(server.spec.server_ref.into_string()),
        model_ref: result
            .prepared
            .model
            .map(|model| model.metadata.model_ref.into_string()),
        provider_model: server.spec.provider_model,
        adapter_ref: result
            .prepared
            .adapter
            .map(|adapter| adapter.metadata.adapter_ref.into_string()),
    })
}

fn chat_role(role: SessionMessageRole) -> Result<ChatRole, RestError> {
    match role {
        SessionMessageRole::System => Ok(ChatRole::System),
        SessionMessageRole::User => Ok(ChatRole::User),
        SessionMessageRole::Assistant => Ok(ChatRole::Assistant),
        SessionMessageRole::Tool => Err(RestError::bad_request(
            "bad_request",
            "session compaction does not support tool messages",
        )),
    }
}
