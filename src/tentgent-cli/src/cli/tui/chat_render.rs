use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame,
};

use super::{
    app::TuiApp,
    chat::{ChatFocus, ChatPhase, ChatSendState, CHAT_MESSAGES_TAIL},
};

pub(super) fn render_chat(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let chunks = if area.height > 12 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Min(8),
                Constraint::Length(5),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Min(6),
                Constraint::Length(4),
            ])
            .split(area)
    };
    render_chat_header(frame, chunks[0], app);
    match app.chat.phase {
        ChatPhase::NoRunningServer => render_no_server(frame, chunks[1], app),
        ChatPhase::ChooseServer => render_server_chooser(frame, chunks[1], app),
        ChatPhase::ChooseSession => render_session_chooser(frame, chunks[1], app),
        ChatPhase::Workspace => render_workspace(frame, chunks[1], app),
    }
    render_chat_footer(frame, chunks[2], app);
}

fn render_chat_header(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("phase: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.chat.phase.label()),
            Span::raw("  "),
            Span::styled("focus: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.chat.focus.label()),
            Span::raw("  "),
            Span::styled("send: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.chat.send_state.label()),
        ]),
        Line::from(vec![
            Span::styled("server: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.chat.selected_server_ref.as_deref().unwrap_or("(none)")),
        ]),
        Line::from(vec![
            Span::styled("session: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.chat.selected_session_ref.as_deref().unwrap_or("(none)")),
            Span::raw("  "),
            Span::styled("adapter: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.chat.selected_adapter_ref.as_deref().unwrap_or("(none)")),
        ]),
        Line::from(vec![
            Span::styled("context: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "next send {} (max_session_messages={}) · transcript tail {}",
                app.chat.context_mode.label(),
                app.chat.context_mode.max_session_messages(),
                CHAT_MESSAGES_TAIL
            )),
        ]),
    ];
    if let Some(error) = &app.chat.last_error {
        lines.push(Line::from(vec![
            Span::styled("last_error: ", Style::default().fg(Color::Red)),
            Span::raw(error.clone()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Chat").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_no_server(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let lines = vec![
        Line::from("No running server is available for chat."),
        Line::from("Slice 3 does not start servers from the TUI."),
        Line::from("Use the CLI or a later server-action slice to start a server, then press r."),
        Line::from(""),
        Line::from(format!("state: {}", app.chat.load_state.label())),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Blocked").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_server_chooser(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let rows = app.chat.servers.iter().enumerate().map(|(index, row)| {
        let selected = index == app.chat.selected_server && app.chat.focus == ChatFocus::Chooser;
        let marker = if index == app.chat.selected_server {
            if selected {
                "●"
            } else {
                ">"
            }
        } else {
            "○"
        };
        Row::new(vec![
            Cell::from(marker),
            Cell::from(row.short_ref.clone()),
            Cell::from(row.label.clone()),
            Cell::from(row.model.clone().unwrap_or_else(|| "-".to_string())),
            Cell::from(
                row.port
                    .map(|port| port.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ])
        .style(selected_style(selected))
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(2),
                Constraint::Length(14),
                Constraint::Length(24),
                Constraint::Min(20),
                Constraint::Length(8),
            ],
        )
        .header(
            Row::new(vec!["", "ref", "server", "model/provider", "port"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .title("Choose Running Server")
                .borders(Borders::ALL),
        ),
        area,
    );
}

fn render_session_chooser(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut items = Vec::new();
    items.push(session_item(
        app.chat.selected_session == 0,
        "● New session",
        "create with selected running server",
    ));
    for (index, row) in app.chat.sessions.iter().enumerate() {
        let selected = app.chat.selected_session == index + 1;
        let marker = if selected { "●" } else { "○" };
        let detail = format!(
            "{} messages · {}",
            row.message_count
                .map(|count| count.to_string())
                .unwrap_or_else(|| "-".to_string()),
            row.updated_at.as_deref().unwrap_or("unknown")
        );
        items.push(session_item(
            selected,
            &format!("{marker} {}", row.title),
            &detail,
        ));
    }
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title("Choose Session")
                .borders(Borders::ALL),
        ),
        area,
    );
}

fn session_item<'a>(selected: bool, label: &str, detail: &str) -> ListItem<'a> {
    let style = selected_style(selected);
    ListItem::new(Line::from(vec![
        Span::styled(label.to_string(), style),
        Span::raw("  "),
        Span::styled(detail.to_string(), Style::default().fg(Color::DarkGray)),
    ]))
}

fn render_workspace(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let wide = area.width >= 120;
    if wide {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(70), Constraint::Length(38)])
            .split(area);
        render_transcript(frame, columns[0], app);
        render_side_pane(frame, columns[1], app);
    } else {
        render_transcript(frame, area, app);
    }
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = Vec::new();
    let take = area.height.saturating_sub(2) as usize;
    let mut rendered = Vec::new();
    for message in &app.chat.transcript {
        let index = message
            .index
            .map(|value| format!("#{value} "))
            .unwrap_or_default();
        rendered.push(Line::from(vec![
            Span::styled(
                format!("{index}{}: ", message.role),
                role_style(&message.role),
            ),
            Span::raw(truncate(
                &message.content,
                area.width.saturating_sub(16) as usize,
            )),
        ]));
        if message.created_at.is_some()
            || message.server_ref.is_some()
            || message.adapter_ref.is_some()
        {
            rendered.push(Line::from(vec![
                Span::styled("  meta: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "{}{}{}",
                        message
                            .created_at
                            .as_deref()
                            .map(|value| format!("at {value} "))
                            .unwrap_or_default(),
                        message
                            .server_ref
                            .as_deref()
                            .map(|value| format!("server {value} "))
                            .unwrap_or_default(),
                        message
                            .adapter_ref
                            .as_deref()
                            .map(|value| format!("adapter {value}"))
                            .unwrap_or_default(),
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }
    if let Some(user) = &app.chat.pending_user {
        rendered.push(Line::from(vec![
            Span::styled("pending user: ", Style::default().fg(Color::Yellow)),
            Span::raw(truncate(user, area.width.saturating_sub(18) as usize)),
        ]));
    }
    if let Some(assistant) = &app.chat.pending_assistant {
        let label = if app.chat.pending_interrupted {
            "assistant interrupted: "
        } else {
            "assistant: "
        };
        rendered.push(Line::from(vec![
            Span::styled(label, Style::default().fg(Color::Cyan)),
            Span::raw(truncate(assistant, area.width.saturating_sub(24) as usize)),
        ]));
    }
    let skip = rendered.len().saturating_sub(take);
    lines.extend(rendered.into_iter().skip(skip));
    if lines.is_empty() {
        lines.push(Line::from(
            "Transcript is empty. Type in the composer and press Enter.",
        ));
    }
    let title = format!(
        "Transcript · {} total{}",
        app.chat
            .total_messages
            .map(|count| count.to_string())
            .unwrap_or_else(|| "-".to_string()),
        if app.chat.transcript_truncated {
            " · tail"
        } else {
            ""
        }
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_side_pane(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = Vec::new();
    if let Some(server) = app.chat.selected_server_row() {
        lines.push(Line::from(Span::styled(
            "Server",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(kv_line("ref", &server.short_ref));
        lines.push(kv_line("running", server.running.to_string()));
        if let Some(host) = &server.host {
            lines.push(kv_line("host", host));
        }
        if let Some(port) = server.port {
            lines.push(kv_line("port", port.to_string()));
        }
        lines.push(kv_line(
            "raw_fields",
            server
                .raw
                .as_object()
                .map(|object| object.len().to_string())
                .unwrap_or_else(|| "0".to_string()),
        ));
    }
    lines.push(Line::from(""));
    if let Some(session) = app.chat.selected_session_row() {
        lines.push(Line::from(Span::styled(
            "Session",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(kv_line("ref", &session.short_ref));
        lines.push(kv_line("title", &session.title));
        if let Some(count) = session.message_count {
            lines.push(kv_line("messages", count.to_string()));
        }
        if let Some(default_server_ref) = &session.default_server_ref {
            lines.push(kv_line("default_server", default_server_ref));
        }
        if let Some(adapter_ref) = &session.adapter_ref {
            lines.push(kv_line("adapter", adapter_ref));
        }
        lines.push(kv_line(
            "raw_fields",
            session
                .raw
                .as_object()
                .map(|object| object.len().to_string())
                .unwrap_or_else(|| "0".to_string()),
        ));
    }
    lines.push(Line::from(""));
    if let Some(adapter) = app.chat.selected_adapter_row() {
        lines.push(Line::from(Span::styled(
            "Adapter",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(kv_line("ref", &adapter.short_ref));
        lines.push(kv_line("label", &adapter.label));
        lines.push(Line::from("compatibility: unverified"));
        lines.push(kv_line(
            "raw_fields",
            adapter
                .raw
                .as_object()
                .map(|object| object.len().to_string())
                .unwrap_or_else(|| "0".to_string()),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Context",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(kv_line("next_send", app.chat.context_mode.label()));
    lines.push(kv_line(
        "max_session_messages",
        app.chat.context_mode.max_session_messages().to_string(),
    ));
    lines.push(kv_line("transcript_tail", CHAT_MESSAGES_TAIL.to_string()));
    lines.push(Line::from("persisted: no"));
    if app.chat.long_context_warning() || app.chat.greeting_loop_warning() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Warnings",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    }
    if app.chat.long_context_warning() {
        lines.push(Line::from(
            "last 50 context may dominate a small local model",
        ));
    }
    if app.chat.greeting_loop_warning() {
        lines.push(Line::from(
            "repeated greeting-like assistant prefixes detected",
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Metadata").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_chat_footer(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = Vec::new();
    if app.chat.phase == ChatPhase::Workspace {
        lines.push(composer_line(app));
    } else {
        lines.push(Line::from(
            "Enter select | n new session | s server | a adapter | r refresh | Esc menu",
        ));
    }
    if app.chat.retry_non_stream.is_some() {
        lines.push(Line::from(
            "Streaming failed before usable output; press f for explicit non-stream retry.",
        ));
    } else {
        lines.push(Line::from(format!(
            "context next send: {} · transcript tail: {} · h cycles context outside composer",
            app.chat.context_mode.label(),
            CHAT_MESSAGES_TAIL
        )));
        lines.push(Line::from(
            "n new session/topic | Tab focus | Esc cancel/back | no server/model mutations in Slice 3",
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Composer").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn composer_line(app: &TuiApp) -> Line<'static> {
    let focused = app.chat.focus == ChatFocus::Composer;
    let locked = !matches!(
        app.chat.send_state,
        ChatSendState::Idle | ChatSendState::Error
    );
    let chars: Vec<char> = app.chat.composer.chars().collect();
    let cursor = app.chat.composer_cursor.min(chars.len());
    let mut spans = vec![Span::styled(
        if locked { "locked: " } else { "prompt: " },
        Style::default().fg(Color::Yellow),
    )];
    for (index, ch) in chars.iter().enumerate() {
        if focused && index == cursor {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().bg(Color::Cyan).fg(Color::Black),
            ));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
    }
    if focused && cursor == chars.len() {
        spans.push(Span::styled(
            " ",
            Style::default().bg(Color::Cyan).fg(Color::Black),
        ));
    }
    if focused {
        spans.push(Span::styled(
            "  Enter send",
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

fn selected_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn role_style(role: &str) -> Style {
    match role {
        "user" => Style::default().fg(Color::Yellow),
        "assistant" => Style::default().fg(Color::Cyan),
        "system" => Style::default().fg(Color::Magenta),
        _ => Style::default(),
    }
}

fn kv_line(label: &str, value: impl ToString) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(Color::Yellow)),
        Span::raw(value.to_string()),
    ])
}

fn truncate(value: &str, max: usize) -> String {
    let max = max.max(16);
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}
