use std::{
    convert::Infallible,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::response::{
    sse::{Event, KeepAlive, Sse},
    IntoResponse, Response,
};
use futures_util::stream;
use serde::Serialize;
use tentgent_kernel::{
    features::{
        adapter::domain::AdapterRefSelector,
        chat::{
            domain::{
                ChatFinishReason, ChatGenerationOptions, ChatMessage, ChatPrompt, ChatRole,
                ChatStreamEvent,
            },
            usecases::{
                ChatCompletionResult, ChatCompletionUseCase, ChatPreparationRequest,
                ChatStreamingUseCase, ChatTargetSelection,
            },
        },
        model::{
            domain::ModelRefSelector,
            usecases::{ModelCatalogReadUseCase, ModelListRequest},
        },
        runtime::domain::PythonRuntimeResolutionInput,
    },
    foundation::{error::KernelError, layout::LayoutResolveMode},
};

use crate::transport::rest::{error::RestError, state::RestState};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChatTransportRequest {
    pub model_ref: String,
    pub adapter_ref: Option<String>,
    pub messages: Vec<ChatTransportMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChatTransportMessage {
    pub role: String,
    pub content: String,
}

pub(super) trait ChatStreamMapper: Send + 'static {
    fn start(&mut self) -> Vec<Event> {
        Vec::new()
    }

    fn delta(&mut self, text: String) -> Vec<Event>;

    fn done(&mut self, finish_reason: ChatFinishReason) -> Vec<Event>;

    fn error(&mut self, code: &str, message: String) -> Vec<Event>;

    fn event(&mut self, event: ChatStreamEvent) -> Vec<Event> {
        match event {
            ChatStreamEvent::Delta { text } => self.delta(text),
            ChatStreamEvent::Done { finish_reason } => self.done(finish_reason),
            ChatStreamEvent::Error { code, message } => self.error(&code, message),
        }
    }
}

pub(super) async fn complete_chat(
    state: RestState,
    request: ChatPreparationRequest,
) -> Result<ChatCompletionResult, RestError> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        handle.block_on(async {
            state
                .app()
                .services()
                .kernel()
                .chat_usecase()
                .complete_chat(request)
                .await
        })
    })
    .await
    .map_err(|error| RestError::internal("chat_failed", format!("chat task failed: {error}")))?
    .map_err(chat_error)
}

pub(super) fn stream_chat_response<M>(
    state: RestState,
    request: ChatPreparationRequest,
    mut mapper: M,
) -> Response
where
    M: ChatStreamMapper,
{
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<Event, Infallible>>();
    std::thread::spawn(move || {
        send_events(&tx, mapper.start());

        let mut sent_delta = false;
        let mut sent_done = false;
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                send_events(
                    &tx,
                    mapper.error(
                        "chat_runtime_failed",
                        format!("failed to create chat stream runtime: {error}"),
                    ),
                );
                return;
            }
        };
        let stream_result = runtime.block_on(async {
            let mut sink = |event| {
                match &event {
                    ChatStreamEvent::Delta { .. } => sent_delta = true,
                    ChatStreamEvent::Done { .. } => sent_done = true,
                    ChatStreamEvent::Error { .. } => {}
                }
                send_events(&tx, mapper.event(event));
            };
            state
                .app()
                .services()
                .kernel()
                .chat_usecase()
                .stream_chat(request, &mut sink)
                .await
        });
        match stream_result {
            Ok(result) => {
                if !sent_delta && !result.response.text.is_empty() {
                    send_events(&tx, mapper.delta(result.response.text));
                }
                if !sent_done {
                    send_events(&tx, mapper.done(result.response.finish_reason));
                }
            }
            Err(error) => {
                send_events(
                    &tx,
                    mapper.error(chat_error_code(&error), error.to_string()),
                );
            }
        }
    });

    let stream = stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|item| (item, rx))
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

pub(super) fn chat_preparation_request(
    state: &RestState,
    request: ChatTransportRequest,
    stream: bool,
) -> Result<ChatPreparationRequest, RestError> {
    let model_selector = model_selector(state, &request.model_ref)?;
    let adapter_selector = match request.adapter_ref {
        Some(value) => Some(AdapterRefSelector::parse(&value).map_err(|err| {
            RestError::bad_request("bad_request", format!("invalid adapter reference: {err}"))
        })?),
        None => None,
    };
    let messages = request
        .messages
        .into_iter()
        .map(chat_message)
        .collect::<Result<Vec<_>, _>>()?;
    let prompt = ChatPrompt::new(messages)
        .map_err(|err| RestError::bad_request("bad_request", err.to_string()))?;

    Ok(ChatPreparationRequest {
        layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        runtime: PythonRuntimeResolutionInput::default(),
        target: ChatTargetSelection::LocalModel {
            model_selector,
            adapter_selector,
        },
        prompt,
        options: ChatGenerationOptions {
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream,
        },
    })
}

fn model_selector(state: &RestState, value: &str) -> Result<ModelRefSelector, RestError> {
    match ModelRefSelector::parse(value) {
        Ok(selector) => Ok(selector),
        Err(_) => model_alias_selector(state, value).map_err(|alias_error| alias_error.error),
    }
}

fn model_alias_selector(
    state: &RestState,
    value: &str,
) -> Result<ModelRefSelector, ModelAliasError> {
    let alias = value.trim();
    if alias.is_empty() {
        return Err(ModelAliasError {
            error: RestError::bad_request("bad_request", "model reference is empty"),
        });
    }
    let result = state
        .app()
        .services()
        .kernel()
        .models()
        .list_models(ModelListRequest {
            layout: state.app().layout_input(LayoutResolveMode::ReadOnly),
        })
        .map_err(|error| ModelAliasError {
            error: RestError::store_lookup("chat_model_failed", error.to_string()),
        })?;

    let matches = result
        .models
        .into_iter()
        .filter(|model| model_alias_matches(alias, model.metadata.source_repo.as_deref()))
        .map(|model| model.metadata.model_ref)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(ModelAliasError {
            error: RestError::not_found(
                "not_found",
                format!("model alias `{alias}` was not found"),
            ),
        }),
        [model_ref] => ModelRefSelector::parse(model_ref.as_str()).map_err(|err| ModelAliasError {
            error: RestError::internal("chat_model_failed", err.to_string()),
        }),
        _ => Err(ModelAliasError {
            error: RestError::conflict(
                "ambiguous_ref",
                format!("model alias `{alias}` matches multiple stored models"),
            ),
        }),
    }
}

fn model_alias_matches(alias: &str, source_repo: Option<&str>) -> bool {
    let Some(source_repo) = source_repo else {
        return false;
    };
    source_repo.eq_ignore_ascii_case(alias)
        || source_repo
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(alias))
}

struct ModelAliasError {
    error: RestError,
}

pub(super) fn finish_reason_str(reason: &ChatFinishReason) -> &str {
    reason.as_str()
}

pub(super) fn sse_json_event(name: Option<&'static str>, payload: &impl Serialize) -> Event {
    let data = serde_json::to_string(payload)
        .unwrap_or_else(|_| r#"{"error":"serialization_failed"}"#.to_string());
    if let Some(name) = name {
        Event::default().event(name).data(data)
    } else {
        Event::default().data(data)
    }
}

pub(super) fn sse_data_event(data: impl Into<String>) -> Event {
    Event::default().data(data.into())
}

pub(super) fn response_id(prefix: &str) -> String {
    format!("{prefix}-{}", unix_timestamp_nanos())
}

pub(super) fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn send_events(
    tx: &tokio::sync::mpsc::UnboundedSender<Result<Event, Infallible>>,
    events: Vec<Event>,
) {
    for event in events {
        let _ = tx.send(Ok(event));
    }
}

fn chat_message(message: ChatTransportMessage) -> Result<ChatMessage, RestError> {
    let role = ChatRole::parse(&message.role)
        .map_err(|err| RestError::bad_request("bad_request", err.to_string()))?;
    ChatMessage::new(role, message.content)
        .map_err(|err| RestError::bad_request("bad_request", err.to_string()))
}

fn chat_error_code(error: &KernelError) -> &'static str {
    match error {
        KernelError::ModelStoreUnavailable(_) => "chat_model_failed",
        KernelError::AdapterStoreUnavailable(_) => "chat_adapter_failed",
        KernelError::UnsupportedTarget(_) => "unsupported_target",
        KernelError::RuntimeStateUnavailable(_) => "chat_runtime_unavailable",
        KernelError::ChatRuntimeUnavailable(_) => "chat_runtime_failed",
        _ => "chat_failed",
    }
}

fn chat_error(error: KernelError) -> RestError {
    match error {
        KernelError::ModelStoreUnavailable(message) => {
            RestError::store_lookup("chat_model_failed", message)
        }
        KernelError::AdapterStoreUnavailable(message) => {
            RestError::store_lookup("chat_adapter_failed", message)
        }
        KernelError::UnsupportedTarget(message) => {
            RestError::bad_request("unsupported_target", message)
        }
        KernelError::RuntimeStateUnavailable(message) => {
            RestError::internal("chat_runtime_unavailable", message)
        }
        KernelError::ChatRuntimeUnavailable(message) => {
            RestError::internal("chat_runtime_failed", message)
        }
        other => RestError::kernel("chat_failed", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_rejects_unknown_role() {
        let error = chat_message(ChatTransportMessage {
            role: "tool".to_string(),
            content: "nope".to_string(),
        })
        .expect_err("tool role should be rejected");

        assert!(format!("{error:?}").contains("RestError"));
    }

    #[test]
    fn chat_message_accepts_user_content() {
        let message = chat_message(ChatTransportMessage {
            role: "user".to_string(),
            content: " hello ".to_string(),
        })
        .expect("message");

        assert_eq!(message.role, ChatRole::User);
        assert_eq!(message.content, "hello");
    }

    #[test]
    fn model_alias_matches_huggingface_repo_or_repo_name() {
        assert!(model_alias_matches(
            "google/gemma-3-1b-it",
            Some("google/gemma-3-1b-it")
        ));
        assert!(model_alias_matches(
            "gemma-3-1b-it",
            Some("google/gemma-3-1b-it")
        ));
        assert!(!model_alias_matches(
            "gemma-3-4b-it",
            Some("google/gemma-3-1b-it")
        ));
    }
}
