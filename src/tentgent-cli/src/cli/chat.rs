use std::process::Stdio;
use std::{
    io::{self, Write},
    path::Path,
};

use miette::{miette, IntoDiagnostic};
use serde_json::json;
use tentgent_core::{
    adapter::AdapterManager,
    model::ModelManager,
    runtime_assets::resolve_runtime_home,
    session::{
        SessionChatContextMessage, SessionManager, SessionMessageInput,
        DEFAULT_SESSION_CONTEXT_MESSAGES, MAX_SESSION_CONTEXT_MESSAGES,
    },
};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::commands::ChatCommand;
use super::python_runtime::{require_python_script, resolve_python_runtime};

pub async fn handle_chat_command(command: ChatCommand) -> miette::Result<()> {
    if command.session_ref.is_none() && command.max_session_messages.is_some() {
        return Err(miette!("--max-session-messages requires --session"));
    }
    if command.session_ref.is_some() {
        return handle_session_chat_command(command).await;
    }

    let python_runtime = resolve_python_runtime()?;
    let python_entrypoint = require_python_script(
        &python_runtime,
        "tentgent-chat-once",
        "python chat entrypoint",
    )?;

    let messages = resolve_messages(&command)?;
    let runtime_home = match &command.home {
        Some(home) => home.clone(),
        None => resolve_runtime_home()
            .map_err(|err| miette!("failed to resolve Tentgent runtime home: {err}"))?
            .to_string_lossy()
            .into_owned(),
    };

    let mut process = Command::new(&python_entrypoint);
    process
        .current_dir(python_runtime.project_dir())
        .arg("--model-ref")
        .arg(&command.model_ref)
        .arg("--home")
        .arg(&runtime_home)
        .env("TENTGENT_HOME", &runtime_home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(max_tokens) = command.max_tokens {
        process.arg("--max-tokens").arg(max_tokens.to_string());
    }

    if let Some(temperature) = command.temperature {
        process.arg("--temperature").arg(temperature.to_string());
    }

    if let Some(adapter_ref) = &command.adapter_ref {
        process.arg("--adapter-ref").arg(adapter_ref);
    }

    if command.stream {
        process.arg("--stream");
    }

    for message in messages {
        process.arg("--message").arg(message);
    }

    let mut child = process.spawn().into_diagnostic()?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| miette!("failed to capture chat runtime stderr"))?;

    let stderr_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        stderr.read_to_end(&mut buffer).await?;
        Ok::<Vec<u8>, std::io::Error>(buffer)
    });

    let status = child.wait().await.into_diagnostic()?;
    let stderr_bytes = stderr_task.await.into_diagnostic()?.into_diagnostic()?;

    if !status.success() {
        let stderr_text = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        if stderr_text.is_empty() {
            return Err(miette!("chat runtime exited with status {status}"));
        }
        return Err(miette!(
            "chat runtime exited with status {status}\n\n{}",
            stderr_text
        ));
    }

    Ok(())
}

async fn handle_session_chat_command(command: ChatCommand) -> miette::Result<()> {
    let python_runtime = resolve_python_runtime()?;
    let python_entrypoint = require_python_script(
        &python_runtime,
        "tentgent-chat-once",
        "python chat entrypoint",
    )?;
    let runtime_home = match &command.home {
        Some(home) => home.clone(),
        None => resolve_runtime_home()
            .map_err(|err| miette!("failed to resolve Tentgent runtime home: {err}"))?
            .to_string_lossy()
            .into_owned(),
    };
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
    let turn = session_manager
        .begin_chat_turn(session_ref, max_session_messages, request_messages)
        .map_err(|err| miette!("failed to prepare session chat turn: {err}"))?;
    let resolved_model_ref = ModelManager::open_readonly_with_home(Some(Path::new(&runtime_home)))
        .and_then(|manager| manager.inspect(&command.model_ref))
        .map(|inspection| inspection.metadata.model_ref)
        .map_err(|err| miette!("failed to resolve model ref for session chat: {err}"))?;
    let effective_adapter_ref = match &command.adapter_ref {
        Some(adapter_ref) => Some(adapter_ref.clone()),
        None => match turn.metadata.adapter_ref.as_deref() {
            Some(adapter_ref) => Some(
                AdapterManager::open_readonly_with_home(Some(Path::new(&runtime_home)))
                    .and_then(|manager| manager.inspect(adapter_ref))
                    .map(|inspection| inspection.metadata.adapter_ref)
                    .map_err(|err| miette!("failed to resolve session adapter_ref: {err}"))?,
            ),
            None => None,
        },
    };

    let mut process = Command::new(&python_entrypoint);
    process
        .current_dir(python_runtime.project_dir())
        .arg("--model-ref")
        .arg(&command.model_ref)
        .arg("--home")
        .arg(&runtime_home)
        .env("TENTGENT_HOME", &runtime_home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(max_tokens) = command.max_tokens {
        process.arg("--max-tokens").arg(max_tokens.to_string());
    }
    if let Some(temperature) = command.temperature {
        process.arg("--temperature").arg(temperature.to_string());
    }
    if let Some(adapter_ref) = &effective_adapter_ref {
        process.arg("--adapter-ref").arg(adapter_ref);
    }
    if command.stream {
        process.arg("--stream");
    }
    for message in &turn.context_messages {
        process.arg("--message").arg(format_cli_message(message));
    }

    let mut child = process.spawn().into_diagnostic()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| miette!("failed to capture chat runtime stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| miette!("failed to capture chat runtime stderr"))?;

    let stream_stdout = command.stream;
    let stdout_task = tokio::spawn(async move {
        let mut collected = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = stdout.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            if stream_stdout {
                print!("{}", String::from_utf8_lossy(&buffer[..read]));
                let _ = io::stdout().flush();
            }
            collected.extend_from_slice(&buffer[..read]);
        }
        Ok::<Vec<u8>, std::io::Error>(collected)
    });
    let stderr_task = tokio::spawn(async move {
        let mut buffer = Vec::new();
        stderr.read_to_end(&mut buffer).await?;
        Ok::<Vec<u8>, std::io::Error>(buffer)
    });

    let status = child.wait().await.into_diagnostic()?;
    let stdout_bytes = stdout_task.await.into_diagnostic()?.into_diagnostic()?;
    let stderr_bytes = stderr_task.await.into_diagnostic()?.into_diagnostic()?;

    if !status.success() {
        let stderr_text = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        if stderr_text.is_empty() {
            return Err(miette!("chat runtime exited with status {status}"));
        }
        return Err(miette!(
            "chat runtime exited with status {status}\n\n{}",
            stderr_text
        ));
    }

    if !command.stream {
        print!("{}", String::from_utf8_lossy(&stdout_bytes));
        io::stdout().flush().into_diagnostic()?;
    }
    let assistant = String::from_utf8_lossy(&stdout_bytes)
        .trim_end_matches(['\r', '\n'])
        .to_string();
    let metadata = json!({
        "route": "cli",
        "server_ref": null,
        "model_ref": resolved_model_ref,
        "provider_model": null,
        "adapter_ref": effective_adapter_ref,
        "finish_reason": "stop",
    });
    turn.append_assistant(assistant, None, effective_adapter_ref, metadata)
        .map_err(|err| miette!("failed to append session transcript: {err}"))?;

    Ok(())
}

fn resolve_messages(command: &ChatCommand) -> miette::Result<Vec<String>> {
    if !command.messages.is_empty() {
        return Ok(command.messages.clone());
    }

    let prompt = prompt_for_message()?;
    Ok(vec![format!("user:{prompt}")])
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

fn format_cli_message(message: &SessionChatContextMessage) -> String {
    format!("{}:{}", message.role, message.content)
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
