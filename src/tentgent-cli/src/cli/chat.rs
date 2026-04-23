use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use miette::{miette, IntoDiagnostic};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::commands::ChatCommand;

pub async fn handle_chat_command(command: ChatCommand) -> miette::Result<()> {
    let python_project = resolve_python_project_dir();
    let python_entrypoint = resolve_python_entrypoint(&python_project)?;
    if !python_entrypoint.exists() {
        return Err(miette!(
            "python chat entrypoint was not found at `{}`",
            python_entrypoint.display()
        ));
    }

    let messages = resolve_messages(&command)?;

    let mut process = Command::new(&python_entrypoint);
    process
        .current_dir(&python_project)
        .arg("--model-ref")
        .arg(&command.model_ref)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(home) = &command.home {
        process.arg("--home").arg(home);
    }

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
    let stderr_bytes = stderr_task
        .await
        .into_diagnostic()?
        .into_diagnostic()?;

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

fn resolve_python_project_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("python/tentgent-daemon")
}

fn resolve_python_entrypoint(project_dir: &Path) -> miette::Result<PathBuf> {
    let entrypoint = project_dir.join(".venv/bin/tentgent-chat-once");
    if entrypoint.exists() {
        return Ok(entrypoint);
    }

    Err(miette!(
        "python chat entrypoint is missing at `{}`; initialize the Python subproject environment first",
        entrypoint.display()
    ))
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
