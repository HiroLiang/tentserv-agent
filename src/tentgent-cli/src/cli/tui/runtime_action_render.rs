use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::runtime_action::{RuntimeActionForm, RuntimeActionState};

pub(super) fn render_runtime_action(frame: &mut Frame<'_>, area: Rect, state: &RuntimeActionState) {
    match state {
        RuntimeActionState::SelectingAction {
            kind,
            actions,
            selected,
        } => {
            let items = actions
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let is_selected = index == *selected;
                    let style = selected_style(is_selected);
                    ListItem::new(Line::from(vec![
                        Span::styled(if is_selected { "●" } else { "○" }, style),
                        Span::raw(" "),
                        Span::styled(action.label(), style),
                        Span::raw("  "),
                        Span::styled(action.detail(), Style::default().fg(Color::DarkGray)),
                    ]))
                })
                .collect::<Vec<_>>();
            frame.render_widget(
                List::new(items).block(
                    Block::default()
                        .title(format!("{} Runtime Actions", kind.label()))
                        .borders(Borders::ALL),
                ),
                area,
            );
        }
        RuntimeActionState::EditingForm {
            kind,
            selected,
            form,
            error,
        } => render_form(
            frame,
            area,
            kind.label(),
            selected.as_ref().map(|row| row.short_ref.as_str()),
            form,
            error.as_deref(),
        ),
        RuntimeActionState::Confirming {
            request,
            typed,
            message,
            ..
        } => {
            let mut lines = vec![
                Line::from(Span::styled(
                    request.action.label(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(message.clone()),
                Line::from(""),
            ];
            if let Some(warning) = &request.warning {
                lines.push(Line::from(vec![
                    Span::styled("warning: ", Style::default().fg(Color::Yellow)),
                    Span::raw(warning.clone()),
                ]));
            }
            if let Some(short_ref) = &request.selected_short_ref {
                lines.push(line_kv("short_ref", short_ref));
            }
            if let Some(full_ref) = &request.selected_ref {
                lines.push(line_kv("full_ref", full_ref));
            }
            lines.push(line_kv(
                "typed",
                if typed.is_empty() {
                    "(empty)".to_string()
                } else {
                    typed.clone()
                },
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(
                "Enter confirms when text matches; Esc cancels local action.",
            ));
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .title("Confirm Runtime Action")
                            .borders(Borders::ALL),
                    )
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        RuntimeActionState::Running {
            request,
            started_at,
            ..
        } => {
            let elapsed = started_at.elapsed().as_secs();
            let mut lines = vec![
                line_kv("action", request.action.label()),
                line_kv("state", "waiting for daemon response"),
                line_kv("elapsed", format!("{elapsed}s")),
            ];
            if let Some(warning) = &request.warning {
                lines.push(line_kv("warning", warning));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(
                "Esc aborts only the local TUI wait; daemon-side work may continue.",
            ));
            if let Some(cli) = &request.cli_hint {
                lines.push(line_kv("cli", cli));
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .title("Runtime Action Running")
                            .borders(Borders::ALL),
                    )
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        RuntimeActionState::Result(result) => {
            let mut lines = vec![
                line_kv("action", result.action.label()),
                line_kv("status", result.status.to_string()),
            ];
            lines.extend(
                result
                    .lines
                    .iter()
                    .take(18)
                    .map(|(key, value)| line_kv(key, value)),
            );
            if matches!(
                result.action,
                super::runtime_action::RuntimeActionKind::ServerStart
            ) {
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "Press Enter/Esc to return, then c to enter Chat.",
                ));
            }
            if result.lines.len() > 18 {
                lines.push(line_kv("truncated", "true"));
            }
            lines.push(line_kv("raw_summary", &result.raw_summary));
            lines.push(Line::from(""));
            lines.push(Line::from("Enter/Esc returns to the table."));
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .title("Runtime Action Result")
                            .borders(Borders::ALL),
                    )
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        RuntimeActionState::Error {
            action,
            message,
            recoverable,
        } => {
            let lines = vec![
                line_kv("action", action.label()),
                line_kv("error", message),
                line_kv("recoverable", recoverable.to_string()),
                Line::from(""),
                Line::from("Enter/Esc returns to the previous table or form."),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .title("Runtime Action Error")
                            .borders(Borders::ALL),
                    )
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        RuntimeActionState::Idle => {}
    }
}

fn render_form(
    frame: &mut Frame<'_>,
    area: Rect,
    section: &str,
    selected_ref: Option<&str>,
    form: &RuntimeActionForm,
    error: Option<&str>,
) {
    let mut lines = vec![
        line_kv("section", section),
        line_kv("action", form.action.label()),
    ];
    if let Some(selected_ref) = selected_ref {
        lines.push(line_kv("selected", selected_ref));
    }
    if let Some(error) = error {
        lines.push(Line::from(vec![
            Span::styled("error: ", Style::default().fg(Color::Red)),
            Span::raw(error.to_string()),
        ]));
    }
    lines.push(Line::from(""));
    if form.fields.is_empty() {
        lines.push(Line::from("No fields. Press Enter to continue."));
    } else {
        for (index, field) in form.fields.iter().enumerate() {
            let selected = index == form.selected_field;
            let style = selected_style(selected);
            let required = if field.spec.required { "*" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(if selected { "● " } else { "○ " }, style),
                Span::styled(format!("{}{}: ", field.spec.name, required), style),
                Span::raw(render_cursor(&field.value, field.cursor, selected)),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "↑/↓ fields | type/edit | Enter submit | Esc cancel",
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Runtime Action Form")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn line_kv(key: impl Into<String>, value: impl ToString) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{}: ", key.into()),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(value.to_string()),
    ])
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

fn render_cursor(value: &str, cursor: usize, selected: bool) -> String {
    if !selected {
        return if value.is_empty() {
            "(empty)".to_string()
        } else {
            value.to_string()
        };
    }
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index == cursor {
            out.push('▌');
        }
        out.push(ch);
    }
    if cursor >= value.chars().count() {
        out.push('▌');
    }
    if out == "▌" {
        "▌".to_string()
    } else {
        out
    }
}
