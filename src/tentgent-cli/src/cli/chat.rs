use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

use miette::{miette, IntoDiagnostic};
use serde_json::json;
use tentgent_core::session::{
    SessionChatContextMessage, SessionCompactionSummary, SessionManager, SessionMessageInput,
    DEFAULT_SESSION_CONTEXT_MESSAGES, MAX_SESSION_CONTEXT_MESSAGES,
};
use tentgent_kernel::features::adapter::domain::AdapterRefSelector;
use tentgent_kernel::features::adapter::infra::FileAdapterCatalogStore;
use tentgent_kernel::features::adapter::usecases::StdAdapterCompatibilityCheckUseCase;
use tentgent_kernel::features::chat::domain::{
    ChatGenerationOptions, ChatMessage, ChatPrompt, ChatRole, ChatStreamEvent,
};
use tentgent_kernel::features::chat::infra::{
    PythonChatOnceRuntimeClient, StdChatAdapterResolver, StdChatModelResolver,
};
use tentgent_kernel::features::chat::usecases::{
    ChatCompletionResult, ChatCompletionUseCase, ChatPreparationRequest, ChatStreamingUseCase,
    ChatTargetSelection, StdChatUseCase,
};
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::model::usecases::StdModelCatalogReadUseCase;
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::StdRuntimeResolutionUseCase;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

use super::commands::ChatCommand;

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
    let runtime_home = resolve_runtime_home_for_cli(command.home.as_deref())?;
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
    let session_manager = SessionManager::new_with_home(Some(Path::new(&runtime_home)))
        .map_err(|err| miette!("failed to open session store: {err}"))?;
    let mut turn = session_manager
        .begin_chat_turn(session_ref, max_session_messages, request_messages)
        .map_err(|err| miette!("failed to prepare session chat turn: {err}"))?;
    let selected_adapter_ref = command
        .adapter_ref
        .clone()
        .or_else(|| turn.metadata.adapter_ref.clone());

    turn.apply_clear_compaction_if_needed()
        .map_err(|err| miette!("failed to compact session transcript: {err}"))?;
    if let Ok(Some(input)) = turn.rolling_context_input() {
        if let Ok(summary) = summarize_with_cli_model(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            &input.prompt_messages,
        )
        .await
        {
            let _ = turn.apply_rolling_context_summary(summary);
        }
    }
    if let Some(input) = turn
        .persisted_compaction_input()
        .map_err(|err| miette!("failed to prepare session compaction: {err}"))?
    {
        let summary = summarize_with_cli_model(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            &input.prompt_messages,
        )
        .await?;
        turn.apply_persisted_compaction_summary(summary)
            .map_err(|err| miette!("failed to compact session transcript: {err}"))?;
    }
    if let Some(input) = turn
        .request_context_summary_input()
        .map_err(|err| miette!("failed to prepare session context summary: {err}"))?
    {
        let summary = summarize_with_cli_model(
            command.home.as_deref(),
            &command.model_ref,
            selected_adapter_ref.as_deref(),
            &input.prompt_messages,
        )
        .await?;
        turn.apply_request_context_summary(summary)
            .map_err(|err| miette!("failed to apply session context summary: {err}"))?;
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
    turn.append_assistant(assistant, None, effective_adapter_ref, metadata)
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
    let runtime_client = PythonChatOnceRuntimeClient::new(&kernel.executable_resolver);
    let chat = StdChatUseCase::new(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
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
    let runtime_client = PythonChatOnceRuntimeClient::new(&kernel.executable_resolver);
    let chat = StdChatUseCase::new(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
    );

    chat.stream_chat(request, sink)
        .await
        .map_err(|err| miette!("chat failed: {err}"))
}

struct CliChatKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_catalog: FileModelCatalogStore,
    adapter_catalog: FileAdapterCatalogStore,
}

impl CliChatKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_catalog: FileModelCatalogStore,
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

fn resolve_runtime_home_for_cli(home: Option<&str>) -> miette::Result<String> {
    let layout = StdRuntimeLayoutResolver
        .resolve(runtime_layout_input(LayoutResolveMode::ReadOnly, home))
        .map_err(|err| miette!("failed to resolve Tentgent runtime home: {err}"))?;
    Ok(layout.home_dir.to_string_lossy().into_owned())
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
        .map(|message| chat_message_from_role_content(&message.role, &message.content))
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
                role: parsed.role,
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
