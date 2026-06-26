use std::{
    io::{self, Write},
    path::PathBuf,
};

use miette::{miette, IntoDiagnostic};
use serde_json::json;
use tentgent_kernel::features::adapter::domain::AdapterRefSelector;
use tentgent_kernel::features::adapter::infra::FileAdapterCatalogStore;
use tentgent_kernel::features::adapter::usecases::StdAdapterCompatibilityCheckUseCase;
use tentgent_kernel::features::chat::domain::{
    ChatGenerationOptions, ChatMessage, ChatPrompt, ChatRole, ChatStreamEvent,
};
use tentgent_kernel::features::chat::infra::{
    PythonChatModelRuntimeClient, StdChatAdapterResolver, StdChatModelResolver,
};
use tentgent_kernel::features::chat::usecases::{
    ChatCompletionResult, ChatCompletionUseCase, ChatPreparationRequest, ChatStreamingUseCase,
    ChatTargetSelection, StdChatUseCase,
};
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::{
    FileModelCapabilityProofStore, FileModelCatalogStore, SystemModelClock,
};
use tentgent_kernel::features::model::usecases::{
    StdModelCatalogReadUseCase, StdModelRuntimeExecutionEvidenceRecorder,
};
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    ModelRuntimeDaemonSupervisor, StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::StdRuntimeResolutionUseCase;
use tentgent_kernel::features::session::domain::{
    SessionChatContextMessage, SessionCompactionSummary, SessionMessageInput, SessionMessageRole,
    DEFAULT_SESSION_CONTEXT_MESSAGES, MAX_SESSION_CONTEXT_MESSAGES,
};
use tentgent_kernel::features::session::usecases::{
    AppendSessionChatAssistantRequest, ApplySessionChatSummaryRequest,
    PrepareSessionChatTurnRequest, SessionChatContextUseCase, SessionChatSummaryScope,
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::{
    commands::ChatCommand,
    session_kernel::{parse_session_selector, session_store_selection_from_str, CliSessionKernel},
};

pub async fn handle_chat_command(command: ChatCommand) -> miette::Result<()> {
    if command.session_ref.is_none() && command.max_session_messages.is_some() {
        return Err(miette!("--max-session-messages requires --session"));
    }
    if command.session_ref.is_some() {
        return handle_session_chat_command(command).await;
    }

    let prompt = resolve_chat_prompt(&command)?;
    let options = chat_generation_options(&command);
    let result = if command.stream {
        let mut sink = |event| print_chat_stream_event(event);
        stream_chat_with_kernel(
            command.home.as_deref(),
            &command.model_ref,
            command.adapter_ref.as_deref(),
            prompt,
            options,
            &mut sink,
        )
        .await?
    } else {
        complete_chat_with_kernel(
            command.home.as_deref(),
            &command.model_ref,
            command.adapter_ref.as_deref(),
            prompt,
            options,
        )
        .await?
    };

    if !command.stream {
        println!("{}", result.response.text);
        io::stdout().flush().into_diagnostic()?;
    }

    Ok(())
}

async fn handle_session_chat_command(command: ChatCommand) -> miette::Result<()> {
    let session_ref = command.session_ref.as_deref().expect("checked session");
    let max_session_messages = command
        .max_session_messages
        .unwrap_or(DEFAULT_SESSION_CONTEXT_MESSAGES);
    if max_session_messages > MAX_SESSION_CONTEXT_MESSAGES {
        return Err(miette!(
            "--max-session-messages must be at most {}",
            MAX_SESSION_CONTEXT_MESSAGES
        ));
    }

    let request_messages = resolve_message_inputs(&command)?;
    let kernel = CliSessionKernel::new();
    let session = kernel.session_usecase();
    let store =
        session_store_selection_from_str(command.home.as_deref(), LayoutResolveMode::Create);
    let selector = parse_session_selector(session_ref)?;
    let mut turn = session
        .prepare_session_chat_turn(PrepareSessionChatTurnRequest {
            store: store.clone(),
            selector: selector.clone(),
            max_session_messages,
            request_messages: request_messages.clone(),
        })
        .map_err(|err| miette!("failed to prepare session chat turn: {err}"))?;
    let selected_adapter_ref = command
        .adapter_ref
        .clone()
        .or_else(|| turn.metadata.adapter_ref.clone());

    if let Some(requirement) = turn.rolling_context.clone() {
        if let Ok(summary) = summarize_with_cli_model(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            requirement.input.prompt_messages(),
        )
        .await
        {
            if let Ok(result) = session.apply_session_chat_summary(ApplySessionChatSummaryRequest {
                store: store.clone(),
                selector: selector.clone(),
                max_session_messages,
                request_messages: request_messages.clone(),
                scope: SessionChatSummaryScope::RollingContext,
                summary,
            }) {
                turn = result.turn;
            }
        }
    }
    if let Some(requirement) = turn.persisted_compaction.clone() {
        let summary = summarize_with_cli_model(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            requirement.input.prompt_messages(),
        )
        .await?;
        turn = session
            .apply_session_chat_summary(ApplySessionChatSummaryRequest {
                store: store.clone(),
                selector: selector.clone(),
                max_session_messages,
                request_messages: request_messages.clone(),
                scope: SessionChatSummaryScope::PersistedCompaction,
                summary,
            })
            .map_err(|err| miette!("failed to compact session transcript: {err}"))?
            .turn;
    }
    if let Some(requirement) = turn.request_context_summary.clone() {
        let summary = summarize_with_cli_model(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            requirement.input.prompt_messages(),
        )
        .await?;
        turn = session
            .apply_session_chat_summary(ApplySessionChatSummaryRequest {
                store: store.clone(),
                selector: selector.clone(),
                max_session_messages,
                request_messages: request_messages.clone(),
                scope: SessionChatSummaryScope::RequestContext,
                summary,
            })
            .map_err(|err| miette!("failed to apply session context summary: {err}"))?
            .turn;
    }

    let prompt = chat_prompt_from_context(&turn.context_messages)?;
    let options = chat_generation_options(&command);
    let result = if command.stream {
        let mut sink = |event| print_chat_stream_event(event);
        stream_chat_with_kernel(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            prompt,
            options,
            &mut sink,
        )
        .await?
    } else {
        complete_chat_with_kernel(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            prompt,
            options,
        )
        .await?
    };

    if !command.stream {
        println!("{}", result.response.text);
        io::stdout().flush().into_diagnostic()?;
    }

    let assistant = result.response.text.clone();
    let resolved_model_ref = canonical_model_ref(&result);
    let effective_adapter_ref = canonical_adapter_ref(&result);
    let metadata = json!({
        "route": "cli",
        "server_ref": null,
        "model_ref": resolved_model_ref,
        "provider_model": null,
        "adapter_ref": effective_adapter_ref,
        "finish_reason": result.response.finish_reason.as_str(),
    });
    session
        .append_session_chat_assistant(AppendSessionChatAssistantRequest {
            store,
            selector,
            request_messages,
            assistant_content: assistant,
            assistant_server_ref: None,
            assistant_adapter_ref: effective_adapter_ref,
            assistant_metadata: metadata,
        })
        .map_err(|err| miette!("failed to append session transcript: {err}"))?;

    Ok(())
}

async fn complete_chat_with_kernel(
    home: Option<&str>,
    model_ref: &str,
    adapter_ref: Option<&str>,
    prompt: ChatPrompt,
    options: ChatGenerationOptions,
) -> miette::Result<ChatCompletionResult> {
    let request = chat_preparation_request(home, model_ref, adapter_ref, prompt, options)?;
    let kernel = CliChatKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdChatModelResolver::new(&model_catalog);
    let adapter_compatibility =
        StdAdapterCompatibilityCheckUseCase::new(&kernel.layout_resolver, &kernel.adapter_catalog);
    let adapter_resolver = StdChatAdapterResolver::new(&adapter_compatibility);
    let runtime_client = PythonChatModelRuntimeClient::new(
        &kernel.executable_resolver,
        &kernel.model_runtime_supervisor,
    );
    let runtime_evidence =
        StdModelRuntimeExecutionEvidenceRecorder::new(&kernel.model_proofs, &kernel.model_clock);
    let chat = StdChatUseCase::new_with_runtime_evidence(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
        &runtime_evidence,
    );

    chat.complete_chat(request)
        .await
        .map_err(|err| miette!("chat failed: {err}"))
}

async fn stream_chat_with_kernel(
    home: Option<&str>,
    model_ref: &str,
    adapter_ref: Option<&str>,
    prompt: ChatPrompt,
    options: ChatGenerationOptions,
    sink: &mut dyn FnMut(ChatStreamEvent),
) -> miette::Result<ChatCompletionResult> {
    let request = chat_preparation_request(home, model_ref, adapter_ref, prompt, options)?;
    let kernel = CliChatKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdChatModelResolver::new(&model_catalog);
    let adapter_compatibility =
        StdAdapterCompatibilityCheckUseCase::new(&kernel.layout_resolver, &kernel.adapter_catalog);
    let adapter_resolver = StdChatAdapterResolver::new(&adapter_compatibility);
    let runtime_client = PythonChatModelRuntimeClient::new(
        &kernel.executable_resolver,
        &kernel.model_runtime_supervisor,
    );
    let runtime_evidence =
        StdModelRuntimeExecutionEvidenceRecorder::new(&kernel.model_proofs, &kernel.model_clock);
    let chat = StdChatUseCase::new_with_runtime_evidence(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
        &runtime_evidence,
    );

    chat.stream_chat(request, sink)
        .await
        .map_err(|err| miette!("chat failed: {err}"))
}

struct CliChatKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_runtime_supervisor: ModelRuntimeDaemonSupervisor,
    model_catalog: FileModelCatalogStore,
    model_proofs: FileModelCapabilityProofStore,
    model_clock: SystemModelClock,
    adapter_catalog: FileAdapterCatalogStore,
}

impl CliChatKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_runtime_supervisor: ModelRuntimeDaemonSupervisor::new(),
            model_catalog: FileModelCatalogStore,
            model_proofs: FileModelCapabilityProofStore,
            model_clock: SystemModelClock,
            adapter_catalog: FileAdapterCatalogStore,
        }
    }
}

fn chat_preparation_request(
    home: Option<&str>,
    model_ref: &str,
    adapter_ref: Option<&str>,
    prompt: ChatPrompt,
    options: ChatGenerationOptions,
) -> miette::Result<ChatPreparationRequest> {
    let model_selector = ModelRefSelector::parse(model_ref)
        .map_err(|err| miette!("failed to parse model ref for chat: {err}"))?;
    let adapter_selector = adapter_ref
        .map(AdapterRefSelector::parse)
        .transpose()
        .map_err(|err| miette!("failed to parse adapter_ref for chat: {err}"))?;

    Ok(ChatPreparationRequest {
        layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home),
        runtime: PythonRuntimeResolutionInput::default(),
        target: ChatTargetSelection::LocalModel {
            model_selector,
            adapter_selector,
        },
        prompt,
        options,
    })
}

fn runtime_layout_input(mode: LayoutResolveMode, home: Option<&str>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: home.map(PathBuf::from),
        data_root_dir: None,
    }
}

fn chat_generation_options(command: &ChatCommand) -> ChatGenerationOptions {
    ChatGenerationOptions {
        max_tokens: command.max_tokens,
        temperature: command.temperature,
        stream: command.stream,
    }
}

fn resolve_messages(command: &ChatCommand) -> miette::Result<Vec<String>> {
    if !command.messages.is_empty() {
        return Ok(command.messages.clone());
    }

    let prompt = prompt_for_message()?;
    Ok(vec![format!("user:{prompt}")])
}

fn resolve_chat_prompt(command: &ChatCommand) -> miette::Result<ChatPrompt> {
    chat_prompt_from_cli_messages(resolve_messages(command)?)
}

fn chat_prompt_from_cli_messages(messages: Vec<String>) -> miette::Result<ChatPrompt> {
    let messages = messages
        .iter()
        .map(|message| {
            let parsed = parse_cli_message(message)?;
            chat_message_from_role_content(&parsed.role, &parsed.content)
        })
        .collect::<miette::Result<Vec<_>>>()?;
    ChatPrompt::new(messages).map_err(|err| miette!("failed to build chat prompt: {err}"))
}

fn chat_prompt_from_context(messages: &[SessionChatContextMessage]) -> miette::Result<ChatPrompt> {
    let messages = messages
        .iter()
        .map(|message| chat_message_from_role_content(message.role.as_str(), &message.content))
        .collect::<miette::Result<Vec<_>>>()?;
    ChatPrompt::new(messages).map_err(|err| miette!("failed to build session chat prompt: {err}"))
}

fn chat_message_from_role_content(role: &str, content: &str) -> miette::Result<ChatMessage> {
    let role = ChatRole::parse(role).map_err(|err| miette!("failed to parse chat role: {err}"))?;
    ChatMessage::new(role, content).map_err(|err| miette!("failed to build chat message: {err}"))
}

fn resolve_message_inputs(command: &ChatCommand) -> miette::Result<Vec<SessionMessageInput>> {
    resolve_messages(command)?
        .into_iter()
        .map(|message| {
            let parsed = parse_cli_message(&message)?;
            Ok(SessionMessageInput {
                role: SessionMessageRole::parse(&parsed.role)
                    .map_err(|err| miette!("failed to parse session message role: {err}"))?,
                content: parsed.content,
                server_ref: None,
                adapter_ref: None,
                metadata: json!({}),
            })
        })
        .collect()
}

struct ParsedCliMessage {
    role: String,
    content: String,
}

fn parse_cli_message(raw: &str) -> miette::Result<ParsedCliMessage> {
    let Some((prefix, remainder)) = raw.split_once(':') else {
        let content = raw.trim().to_string();
        if content.is_empty() {
            return Err(miette!("message content must not be empty"));
        }
        return Ok(ParsedCliMessage {
            role: "user".to_string(),
            content,
        });
    };
    let role = prefix.trim().to_lowercase();
    if role == "tool" {
        return Err(miette!(
            "session-aware chat message role must be one of: system, user, assistant"
        ));
    }
    if !matches!(role.as_str(), "system" | "user" | "assistant") {
        let content = raw.trim().to_string();
        if content.is_empty() {
            return Err(miette!("message content must not be empty"));
        }
        return Ok(ParsedCliMessage {
            role: "user".to_string(),
            content,
        });
    }
    let content = remainder.trim().to_string();
    if content.is_empty() {
        return Err(miette!("message for role `{role}` must not be empty"));
    }
    Ok(ParsedCliMessage { role, content })
}

async fn summarize_with_cli_model(
    home: Option<&str>,
    model_ref: &str,
    adapter_ref: Option<&str>,
    prompt_messages: &[SessionChatContextMessage],
) -> miette::Result<SessionCompactionSummary> {
    let prompt = chat_prompt_from_context(prompt_messages)?;
    let result = complete_chat_with_kernel(
        home,
        model_ref,
        adapter_ref,
        prompt,
        ChatGenerationOptions::default(),
    )
    .await?;
    if result.response.text.trim().is_empty() {
        return Err(miette!("compaction runtime returned an empty summary"));
    }
    let model_ref = canonical_model_ref(&result);
    let adapter_ref = canonical_adapter_ref(&result);

    Ok(SessionCompactionSummary {
        content: result.response.text,
        server_ref: None,
        model_ref,
        provider_model: None,
        adapter_ref,
    })
}

fn canonical_model_ref(result: &ChatCompletionResult) -> Option<String> {
    result
        .prepared
        .model
        .as_ref()
        .map(|model| model.metadata.model_ref.to_string())
}

fn canonical_adapter_ref(result: &ChatCompletionResult) -> Option<String> {
    result
        .prepared
        .adapter
        .as_ref()
        .map(|adapter| adapter.metadata.adapter_ref.to_string())
}

fn print_chat_stream_event(event: ChatStreamEvent) {
    match event {
        ChatStreamEvent::Delta { text } => {
            print!("{text}");
            let _ = io::stdout().flush();
        }
        ChatStreamEvent::Done { .. } => {}
        ChatStreamEvent::Error { message, .. } => {
            let _ = writeln!(io::stderr(), "{message}");
        }
    }
}

fn prompt_for_message() -> miette::Result<String> {
    print!("Message: ");
    io::stdout().flush().into_diagnostic()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).into_diagnostic()?;
    let message = input.trim().to_string();
    if message.is_empty() {
        return Err(miette!("message input must not be empty"));
    }

    Ok(message)
}
