use std::io::{self, Write};
use std::process::Stdio;

use miette::{miette, IntoDiagnostic};
use tentgent_core::runtime_assets::resolve_runtime_home;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::commands::ChatCommand;
use super::python_runtime::{require_python_script, resolve_python_runtime};

pub async fn handle_chat_command(command: ChatCommand) -> miette::Result<()> {
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

fn resolve_messages(command: &ChatCommand) -> miette::Result<Vec<String>> {
    if !command.messages.is_empty() {
        return Ok(command.messages.clone());
    }

    let prompt = prompt_for_message()?;
    Ok(vec![format!("user:{prompt}")])
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
