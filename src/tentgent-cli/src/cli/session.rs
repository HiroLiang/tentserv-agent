use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use serde_json::Value;
use tentgent_core::{
    server::{ServerInspection, ServerManager},
    session::{
        SessionAppendOutcome, SessionCompactionInput, SessionCompactionOutcome,
        SessionCompactionSummary, SessionCreateRequest, SessionInspection, SessionManager,
        SessionMessageInput, SessionMessages, SessionOptionalStringPatch, SessionRemovalOutcome,
        SessionSummary, SessionUpdateRequest,
    },
};

use super::commands::SessionCommands;

pub async fn handle_session_command(action: SessionCommands) -> miette::Result<()> {
    match action {
        SessionCommands::Ls { home } => {
            let manager = SessionManager::open_readonly(home.as_deref()).into_diagnostic()?;
            let sessions = manager.list().into_diagnostic()?;
            render_session_list(&sessions);
        }
        SessionCommands::Inspect { reference, home } => {
            let manager = SessionManager::open_readonly(home.as_deref()).into_diagnostic()?;
            let inspection = manager.inspect(&reference).into_diagnostic()?;
            render_session_inspection("Session inspection", &inspection);
        }
        SessionCommands::Messages {
            reference,
            tail,
            home,
        } => {
            if tail == 0 {
                return Err(miette!("--tail must be greater than zero"));
            }
            let manager = SessionManager::open_readonly(home.as_deref()).into_diagnostic()?;
            let messages = manager.messages(&reference, tail).into_diagnostic()?;
            render_session_messages(&messages);
        }
        SessionCommands::Create {
            title,
            default_server,
            adapter,
            tags,
            home,
        } => {
            let manager = SessionManager::new_with_home(home.as_deref()).into_diagnostic()?;
            let inspection = manager
                .create(SessionCreateRequest {
                    title,
                    default_server_ref: default_server,
                    adapter_ref: adapter,
                    tags,
                    messages: Vec::new(),
                })
                .into_diagnostic()?;
            render_session_inspection("Session created", &inspection);
        }
        SessionCommands::Update {
            reference,
            title,
            clear_title,
            default_server,
            clear_default_server,
            adapter,
            clear_adapter,
            tags,
            clear_tags,
            home,
        } => {
            let manager = SessionManager::new_with_home(home.as_deref()).into_diagnostic()?;
            let request = SessionUpdateRequest {
                title: optional_patch(title, clear_title),
                default_server_ref: optional_patch(default_server, clear_default_server),
                adapter_ref: optional_patch(adapter, clear_adapter),
                tags: if clear_tags {
                    Some(Vec::new())
                } else if tags.is_empty() {
                    None
                } else {
                    Some(tags)
                },
            };
            let inspection = manager.update(&reference, request).into_diagnostic()?;
            render_session_inspection("Session updated", &inspection);
        }
        SessionCommands::Append {
            reference,
            role,
            content,
            metadata_json,
            compaction_server,
            home,
        } => {
            let metadata = parse_metadata_json(&metadata_json)?;
            let manager = SessionManager::new_with_home(home.as_deref()).into_diagnostic()?;
            let mut turn = manager
                .begin_append_messages(
                    &reference,
                    vec![SessionMessageInput {
                        role,
                        content,
                        server_ref: None,
                        adapter_ref: None,
                        metadata,
                    }],
                )
                .into_diagnostic()?;
            if let Some(input) = turn.compaction_input().into_diagnostic()? {
                let server_ref = compaction_server
                    .as_deref()
                    .or_else(|| turn.default_server_ref())
                    .ok_or_else(|| {
                        miette!(
                            "session append requires --compaction-server or a session default server"
                        )
                    })?;
                let server = resolve_running_server(home.as_deref(), server_ref)?;
                let summary = summarize_with_server(&server, &input).await?;
                turn.apply_compaction_summary(summary).into_diagnostic()?;
            }
            let outcome = turn.append_after_compaction().into_diagnostic()?;
            render_append_outcome(&outcome);
        }
        SessionCommands::Compact {
            reference,
            server,
            keep_recent,
            instructions,
            home,
        } => {
            let manager = SessionManager::new_with_home(home.as_deref()).into_diagnostic()?;
            let turn = manager
                .begin_compaction(&reference, keep_recent, instructions.clone())
                .into_diagnostic()?;
            let Some(input) = turn.compaction_input(instructions).into_diagnostic()? else {
                render_compaction_outcome(&turn.no_op_outcome());
                return Ok(());
            };
            let server_ref = server
                .as_deref()
                .or_else(|| turn.default_server_ref())
                .ok_or_else(|| {
                    miette!("session compact requires --server or a session default server")
                })?;
            let server = resolve_running_server(home.as_deref(), server_ref)?;
            let summary = summarize_with_server(&server, &input).await?;
            let outcome = turn.apply_summary(summary).into_diagnostic()?;
            render_compaction_outcome(&outcome);
        }
        SessionCommands::Rm { reference, home } => {
            let manager = SessionManager::new_with_home(home.as_deref()).into_diagnostic()?;
            let outcome = manager.remove(&reference).into_diagnostic()?;
            render_removal_outcome(&outcome);
        }
    }

    Ok(())
}

fn optional_patch(value: Option<String>, clear: bool) -> SessionOptionalStringPatch {
    if clear {
        SessionOptionalStringPatch::Clear
    } else {
        value
            .map(SessionOptionalStringPatch::Set)
            .unwrap_or(SessionOptionalStringPatch::Unchanged)
    }
}

fn parse_metadata_json(input: &str) -> miette::Result<Value> {
    let value = serde_json::from_str::<Value>(input)
        .map_err(|error| miette!("--metadata-json must be a JSON object: {error}"))?;
    if !value.is_object() {
        return Err(miette!("--metadata-json must be a JSON object"));
    }
    Ok(value)
}

fn resolve_running_server(
    home: Option<&std::path::Path>,
    reference: &str,
) -> miette::Result<ServerInspection> {
    let manager = ServerManager::open_readonly(home).into_diagnostic()?;
    let inspection = manager.inspect(reference).into_diagnostic()?;
    if !inspection.running {
        return Err(miette!(
            "server `{}` is not running",
            inspection.spec.short_ref
        ));
    }
    Ok(inspection)
}

async fn summarize_with_server(
    server: &ServerInspection,
    input: &SessionCompactionInput,
) -> miette::Result<SessionCompactionSummary> {
    let url = format!("http://{}:{}/v1/chat", server.spec.host, server.spec.port);
    let body = serde_json::json!({
        "messages": input
            .prompt_messages
            .iter()
            .map(|message| serde_json::json!({
                "role": message.role,
                "content": message.content,
            }))
            .collect::<Vec<_>>(),
        "stream": false,
    });
    let response = reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .into_diagnostic()?;
    if !response.status().is_success() {
        return Err(miette!(
            "compaction server returned HTTP status {}",
            response.status()
        ));
    }
    let payload = response.json::<Value>().await.into_diagnostic()?;
    let text = payload
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("compaction server response did not contain string `text`"))?;
    if text.trim().is_empty() {
        return Err(miette!("compaction server returned an empty summary"));
    }
    Ok(SessionCompactionSummary {
        content: text.to_string(),
        server_ref: Some(server.spec.server_ref.clone()),
        model_ref: server.spec.model_ref.clone(),
        provider_model: server.spec.provider_model.clone(),
        adapter_ref: None,
    })
}

fn render_session_list(sessions: &[SessionSummary]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Local sessions").bold()
    );
    if sessions.is_empty() {
        println!(
            "{} No local sessions are stored yet.\n",
            style("empty").yellow().bold()
        );
        return;
    }

    let mut table = base_table();
    table.set_header(vec![
        "short_ref",
        "title",
        "messages",
        "updated_at",
        "default_server",
        "adapter",
        "tags",
    ]);
    for session in sessions {
        let metadata = &session.metadata;
        let tags = if metadata.tags.is_empty() {
            "-".to_string()
        } else {
            metadata.tags.join(",")
        };
        table.add_row(vec![
            Cell::new(&metadata.short_ref),
            Cell::new(display_option(metadata.title.as_deref())),
            Cell::new(metadata.message_count),
            Cell::new(&metadata.updated_at),
            Cell::new(display_short_option(metadata.default_server_ref.as_deref())),
            Cell::new(display_short_option(metadata.adapter_ref.as_deref())),
            Cell::new(tags),
        ]);
    }
    println!("{table}");
    println!();
}

fn render_session_inspection(title: &str, inspection: &SessionInspection) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style(title).bold(),
        style(&inspection.metadata.short_ref).bold()
    );

    let metadata = &inspection.metadata;
    let mut table = base_table();
    table.add_row(vec![
        Cell::new("session_ref"),
        Cell::new(&metadata.session_ref),
    ]);
    table.add_row(vec![Cell::new("short_ref"), Cell::new(&metadata.short_ref)]);
    table.add_row(vec![
        Cell::new("title"),
        Cell::new(display_option(metadata.title.as_deref())),
    ]);
    table.add_row(vec![
        Cell::new("created_at"),
        Cell::new(&metadata.created_at),
    ]);
    table.add_row(vec![
        Cell::new("updated_at"),
        Cell::new(&metadata.updated_at),
    ]);
    table.add_row(vec![
        Cell::new("message_count"),
        Cell::new(metadata.message_count),
    ]);
    table.add_row(vec![
        Cell::new("default_server_ref"),
        Cell::new(display_option(metadata.default_server_ref.as_deref())),
    ]);
    table.add_row(vec![
        Cell::new("adapter_ref"),
        Cell::new(display_option(metadata.adapter_ref.as_deref())),
    ]);
    table.add_row(vec![Cell::new("tags"), Cell::new(metadata.tags.join(","))]);
    table.add_row(vec![
        Cell::new("store path"),
        Cell::new(inspection.store_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("messages path"),
        Cell::new(inspection.messages_path.display().to_string()),
    ]);
    if !inspection.warnings.is_empty() {
        table.add_row(vec![
            Cell::new("warnings"),
            Cell::new(
                inspection
                    .warnings
                    .iter()
                    .map(|warning| format!("{}: {}", warning.code, warning.message))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ]);
    }
    println!("{table}");
    println!();
}

fn render_session_messages(messages: &SessionMessages) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Session messages").bold(),
        style(&messages.short_ref).bold()
    );
    println!(
        "{} total={}, tail={}, truncated={}",
        style("==>").cyan().bold(),
        messages.total_messages,
        messages.tail,
        messages.truncated
    );
    if messages.messages.is_empty() {
        println!("{} No messages found.\n", style("empty").yellow().bold());
        return;
    }
    for message in &messages.messages {
        println!(
            "{}",
            style(format!("--- message {} ---", message.index)).bold()
        );
        println!("role: {}", message.role);
        println!("created_at: {}", message.created_at);
        if let Some(server_ref) = message.server_ref.as_deref() {
            println!("server_ref: {server_ref}");
        }
        if let Some(adapter_ref) = message.adapter_ref.as_deref() {
            println!("adapter_ref: {adapter_ref}");
        }
        if should_render_metadata(&message.metadata) {
            let metadata = serde_json::to_string(&message.metadata)
                .unwrap_or_else(|_| message.metadata.to_string());
            println!("metadata: {metadata}");
        }
        println!("content:");
        println!("{}", message.content);
        println!();
    }
    println!();
}

fn should_render_metadata(metadata: &Value) -> bool {
    !metadata
        .as_object()
        .map(|object| object.is_empty())
        .unwrap_or(false)
}

fn render_append_outcome(outcome: &SessionAppendOutcome) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Session message appended").bold(),
        style(&outcome.metadata.short_ref).bold()
    );
    let mut table = base_table();
    table.set_header(vec!["index", "role", "created_at"]);
    for appended in &outcome.appended {
        table.add_row(vec![
            Cell::new(appended.index),
            Cell::new(&appended.role),
            Cell::new(&appended.created_at),
        ]);
    }
    println!("{table}");
    println!("message_count: {}", outcome.metadata.message_count);
    println!();
}

fn render_compaction_outcome(outcome: &SessionCompactionOutcome) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Session compacted").bold(),
        style(&outcome.metadata.short_ref).bold()
    );

    let mut table = base_table();
    table.add_row(vec![Cell::new("compacted"), Cell::new(outcome.compacted)]);
    table.add_row(vec![
        Cell::new("message_count"),
        Cell::new(outcome.metadata.message_count),
    ]);
    table.add_row(vec![
        Cell::new("source_message_count"),
        Cell::new(outcome.source_message_count),
    ]);
    table.add_row(vec![
        Cell::new("replaced_message_count"),
        Cell::new(outcome.replaced_message_count),
    ]);
    table.add_row(vec![
        Cell::new("kept_recent_messages"),
        Cell::new(outcome.kept_recent_messages),
    ]);
    table.add_row(vec![
        Cell::new("summary_index"),
        Cell::new(
            outcome
                .summary_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
    ]);
    println!("{table}");
    println!();
}

fn render_removal_outcome(outcome: &SessionRemovalOutcome) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Session removed").bold(),
        style(&outcome.inspection.metadata.short_ref).bold()
    );
    println!(
        "{} {}",
        style("removed").red().bold(),
        outcome.inspection.store_path.display()
    );
    println!();
}

fn display_option(value: Option<&str>) -> &str {
    value.filter(|value| !value.is_empty()).unwrap_or("-")
}

fn display_short_option(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(12).collect())
        .unwrap_or_else(|| "-".to_string())
}

fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS);
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_list_shortens_default_server_and_adapter_refs() {
        let long_ref = "5ab47943b50d1716340db7e1a80f4feac0febd26fbd08b3552f26f3128707626";

        assert_eq!(display_short_option(Some(long_ref)), "5ab47943b50d");
        assert_eq!(display_short_option(None), "-");
    }
}
