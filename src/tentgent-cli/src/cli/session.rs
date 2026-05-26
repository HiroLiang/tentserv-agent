use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use serde_json::Value;
use tentgent_kernel::{
    features::{
        server::domain::ServerInspection,
        session::{
            domain::{
                SessionAppendOutcome, SessionChatContextMessage, SessionCompactionOutcome,
                SessionCompactionSummary, SessionInspection, SessionMessageInput,
                SessionMessageRole, SessionMessages, SessionOptionalStringPatch,
                SessionRemovalOutcome, SessionStorageLocation, SessionSummary,
            },
            usecases::{
                AppendSessionMessagesRequest, AppendSessionMessagesResult,
                ApplySessionAppendCompactionRequest, ApplySessionCompactionRequest,
                CreateSessionRequest, PrepareSessionCompactionRequest,
                PrepareSessionCompactionResult, RemoveSessionRequest, SessionCatalogReadUseCase,
                SessionCompactionUseCase, SessionMutationUseCase, SessionSummaryRequirement,
                UpdateSessionRequest,
            },
        },
    },
    foundation::layout::LayoutResolveMode,
};

use super::{
    commands::SessionCommands,
    session_kernel::{parse_session_selector, session_store_selection, CliSessionKernel},
};

pub async fn handle_session_command(action: SessionCommands) -> miette::Result<()> {
    match action {
        SessionCommands::Ls { home } => {
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let result = usecase
                .list_sessions(
                    tentgent_kernel::features::session::usecases::SessionListRequest {
                        store: session_store_selection(
                            home.as_deref(),
                            LayoutResolveMode::ReadOnly,
                        ),
                    },
                )
                .into_diagnostic()?;
            render_session_list(&result.sessions);
        }
        SessionCommands::Inspect { reference, home } => {
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let inspection = usecase
                .inspect_session(
                    tentgent_kernel::features::session::usecases::SessionInspectRequest {
                        store: session_store_selection(
                            home.as_deref(),
                            LayoutResolveMode::ReadOnly,
                        ),
                        selector: parse_session_selector(&reference)?,
                    },
                )
                .into_diagnostic()?
                .inspection;
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
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let messages = usecase
                .read_session_messages(
                    tentgent_kernel::features::session::usecases::SessionMessagesRequest {
                        store: session_store_selection(
                            home.as_deref(),
                            LayoutResolveMode::ReadOnly,
                        ),
                        selector: parse_session_selector(&reference)?,
                        tail,
                    },
                )
                .into_diagnostic()?
                .messages;
            render_session_messages(&messages);
        }
        SessionCommands::Create {
            title,
            default_server,
            adapter,
            tags,
            home,
        } => {
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let inspection = usecase
                .create_session(CreateSessionRequest {
                    store: session_store_selection(home.as_deref(), LayoutResolveMode::Create),
                    create: tentgent_kernel::features::session::domain::SessionCreateRequest {
                        title,
                        default_server_ref: default_server,
                        adapter_ref: adapter,
                        tags,
                        messages: Vec::new(),
                    },
                })
                .into_diagnostic()?;
            render_session_inspection("Session created", &inspection.inspection);
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
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let request = UpdateSessionRequest {
                store: session_store_selection(home.as_deref(), LayoutResolveMode::Create),
                selector: parse_session_selector(&reference)?,
                update: tentgent_kernel::features::session::domain::SessionUpdateRequest {
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
                },
            };
            let inspection = usecase
                .update_session(request)
                .into_diagnostic()?
                .inspection;
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
            let role = SessionMessageRole::parse(&role)
                .map_err(|err| miette!("invalid session message role: {err}"))?;
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let store = session_store_selection(home.as_deref(), LayoutResolveMode::Create);
            let selector = parse_session_selector(&reference)?;
            let messages = vec![SessionMessageInput {
                role,
                content,
                server_ref: None,
                adapter_ref: None,
                metadata,
            }];
            match usecase
                .append_session_messages(AppendSessionMessagesRequest {
                    store: store.clone(),
                    selector: selector.clone(),
                    messages: messages.clone(),
                })
                .into_diagnostic()?
            {
                AppendSessionMessagesResult::Appended {
                    outcome,
                    clear_compaction,
                    ..
                } => {
                    if let Some(compaction) = clear_compaction {
                        render_compaction_outcome(&compaction);
                    }
                    render_append_outcome(&outcome);
                }
                AppendSessionMessagesResult::CompactionRequired { requirement, .. } => {
                    let summary = summarize_requirement_with_server(
                        &kernel,
                        home.as_deref(),
                        &requirement,
                        compaction_server.as_deref(),
                        "session append requires --compaction-server or a session default server",
                    )
                    .await?;
                    let result = usecase
                        .apply_session_append_compaction(ApplySessionAppendCompactionRequest {
                            store,
                            selector,
                            messages,
                            summary,
                        })
                        .into_diagnostic()?;
                    if let Some(compaction) = result.compaction {
                        render_compaction_outcome(&compaction);
                    }
                    render_append_outcome(&result.outcome);
                }
            }
        }
        SessionCommands::Compact {
            reference,
            server,
            keep_recent,
            instructions,
            home,
        } => {
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let store = session_store_selection(home.as_deref(), LayoutResolveMode::Create);
            let selector = parse_session_selector(&reference)?;
            match usecase
                .prepare_session_compaction(PrepareSessionCompactionRequest {
                    store: store.clone(),
                    selector: selector.clone(),
                    keep_recent_messages: keep_recent,
                    instructions: instructions.clone(),
                })
                .into_diagnostic()?
            {
                PrepareSessionCompactionResult::NoOp { outcome, .. } => {
                    render_compaction_outcome(&outcome);
                }
                PrepareSessionCompactionResult::SummaryRequired { requirement, .. } => {
                    let summary = summarize_requirement_with_server(
                        &kernel,
                        home.as_deref(),
                        &requirement,
                        server.as_deref(),
                        "session compact requires --server or a session default server",
                    )
                    .await?;
                    let outcome = usecase
                        .apply_session_compaction(ApplySessionCompactionRequest {
                            store,
                            selector,
                            keep_recent_messages: keep_recent,
                            instructions,
                            summary,
                        })
                        .into_diagnostic()?
                        .outcome;
                    render_compaction_outcome(&outcome);
                }
            }
        }
        SessionCommands::Rm { reference, home } => {
            let kernel = CliSessionKernel::new();
            let usecase = kernel.session_usecase();
            let outcome = usecase
                .remove_session(RemoveSessionRequest {
                    store: session_store_selection(home.as_deref(), LayoutResolveMode::Create),
                    selector: parse_session_selector(&reference)?,
                })
                .into_diagnostic()?
                .outcome;
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

async fn summarize_requirement_with_server(
    kernel: &CliSessionKernel,
    home: Option<&std::path::Path>,
    requirement: &SessionSummaryRequirement,
    server_override: Option<&str>,
    missing_server_message: &str,
) -> miette::Result<SessionCompactionSummary> {
    let server_ref = server_override
        .or(requirement.default_server_ref.as_deref())
        .ok_or_else(|| miette!("{missing_server_message}"))?;
    let server = kernel
        .inspect_running_server(home, server_ref)
        .into_diagnostic()?;
    summarize_with_server(&server, requirement.input.prompt_messages()).await
}

async fn summarize_with_server(
    server: &ServerInspection,
    prompt_messages: &[SessionChatContextMessage],
) -> miette::Result<SessionCompactionSummary> {
    let url = format!(
        "http://{}:{}/v1/chat",
        server.spec.host,
        server.effective_port()
    );
    let body = serde_json::json!({
        "messages": prompt_messages
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
        server_ref: Some(server.spec.server_ref.to_string()),
        model_ref: server.spec.model_ref.as_ref().map(ToString::to_string),
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
        Cell::new(metadata.session_ref.as_str()),
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
    add_location_rows(&mut table, &inspection.location);
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
        location_label(&outcome.inspection.location)
    );
    println!();
}

fn add_location_rows(table: &mut Table, location: &SessionStorageLocation) {
    match location {
        SessionStorageLocation::File(paths) => {
            table.add_row(vec![
                Cell::new("store path"),
                Cell::new(paths.store_path.display().to_string()),
            ]);
            table.add_row(vec![
                Cell::new("messages path"),
                Cell::new(paths.messages_path.display().to_string()),
            ]);
        }
        SessionStorageLocation::External { backend, locator } => {
            table.add_row(vec![Cell::new("store backend"), Cell::new(backend)]);
            table.add_row(vec![
                Cell::new("store locator"),
                Cell::new(display_option(locator.as_deref())),
            ]);
        }
    }
}

fn location_label(location: &SessionStorageLocation) -> String {
    match location {
        SessionStorageLocation::File(paths) => paths.store_path.display().to_string(),
        SessionStorageLocation::External { backend, locator } => locator
            .as_ref()
            .map(|locator| format!("{backend}:{locator}"))
            .unwrap_or_else(|| backend.clone()),
    }
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
