use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use serde_json::Value;
use tentgent_core::session::{
    SessionAppendOutcome, SessionCreateRequest, SessionInspection, SessionManager,
    SessionMessageInput, SessionMessages, SessionOptionalStringPatch, SessionRemovalOutcome,
    SessionSummary, SessionUpdateRequest,
};

use super::commands::SessionCommands;

pub fn handle_session_command(action: SessionCommands) -> miette::Result<()> {
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
            home,
        } => {
            let metadata = parse_metadata_json(&metadata_json)?;
            let manager = SessionManager::new_with_home(home.as_deref()).into_diagnostic()?;
            let outcome = manager
                .append_messages(
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
            render_append_outcome(&outcome);
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
            Cell::new(display_option(metadata.default_server_ref.as_deref())),
            Cell::new(display_option(metadata.adapter_ref.as_deref())),
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
    let mut table = base_table();
    table.set_header(vec!["index", "role", "created_at", "content"]);
    for message in &messages.messages {
        table.add_row(vec![
            Cell::new(message.index),
            Cell::new(&message.role),
            Cell::new(&message.created_at),
            Cell::new(&message.content),
        ]);
    }
    println!("{table}");
    println!();
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

fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS);
    table
}
