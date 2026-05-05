use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::{
    navigator::{NavigatorListKind, NavigatorRow, NavigatorState},
    runtime_action::{RuntimeActionForm, RuntimeActionState},
    runtime_wizard::{
        RuntimePickerMode, RuntimePreviewStatus, RuntimeWizardAdvancedChoice, RuntimeWizardBackend,
        RuntimeWizardReviewRow, RuntimeWizardState, RuntimeWizardStep,
    },
};

pub(super) fn render_runtime_action(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &RuntimeActionState,
    navigator: &NavigatorState,
) {
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
        RuntimeActionState::Wizard(wizard) => {
            render_wizard(frame, area, wizard, navigator, None);
        }
        RuntimeActionState::WizardPreviewRunning {
            wizard, started_at, ..
        } => {
            render_wizard(
                frame,
                area,
                wizard,
                navigator,
                Some(format!(
                    "preview running for {}s",
                    started_at.elapsed().as_secs()
                )),
            );
        }
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

fn render_wizard(
    frame: &mut Frame<'_>,
    area: Rect,
    wizard: &RuntimeWizardState,
    navigator: &NavigatorState,
    running: Option<String>,
) {
    let title = format!("{} · {}", wizard.flow.label(), wizard.step.label());
    match wizard.step {
        RuntimeWizardStep::PickModel | RuntimeWizardStep::PickDataset => {
            render_picker_step(frame, area, wizard, navigator, &title);
        }
        RuntimeWizardStep::ServerConfig => render_server_config(frame, area, wizard, &title),
        RuntimeWizardStep::PickBackend => render_backend_step(frame, area, wizard, &title),
        RuntimeWizardStep::PlanBasics => render_plan_basics(frame, area, wizard, &title),
        RuntimeWizardStep::AdvancedChoice => render_advanced_choice(frame, area, wizard, &title),
        RuntimeWizardStep::AdvancedFields => render_advanced_fields(frame, area, wizard, &title),
        RuntimeWizardStep::Review => render_review(frame, area, wizard, &title, running),
    }
}

fn render_picker_step(
    frame: &mut Frame<'_>,
    area: Rect,
    wizard: &RuntimeWizardState,
    navigator: &NavigatorState,
    title: &str,
) {
    let Some(picker) = &wizard.picker else {
        return;
    };
    let chunks = if area.width > 110 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area)
    };
    let state = navigator.state(picker.kind);
    let rows = picker_visible_rows(navigator, picker.kind, &picker.filter);
    let mut items = Vec::new();
    if picker.mode == RuntimePickerMode::Manual {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                "● manual: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(render_cursor(
                &picker.manual_value,
                picker.manual_cursor,
                true,
            )),
        ])));
    } else if rows.is_empty() {
        items.push(ListItem::new(Line::from(
            "No local rows. Press m for advanced manual ref or r refresh.",
        )));
    } else {
        for (index, row) in rows.iter().enumerate().take(200) {
            let selected = index == picker.selected_index;
            let style = selected_style(selected);
            items.push(ListItem::new(Line::from(vec![
                Span::styled(if selected { "● " } else { "○ " }, style),
                Span::styled(&row.short_ref, style),
                Span::raw("  "),
                Span::styled(row.columns.join("  "), Style::default().fg(Color::DarkGray)),
            ])));
        }
    }
    frame.render_widget(
        List::new(items).block(Block::default().title(title).borders(Borders::ALL)),
        chunks[0],
    );

    let selected_row = if picker.mode == RuntimePickerMode::Local {
        rows.get(picker.selected_index).copied()
    } else {
        None
    };
    let mut detail = vec![
        line_kv("source", picker.mode.label()),
        line_kv("list", picker.kind.label()),
        line_kv("load_state", state.load_state.label()),
        line_kv(
            "filter",
            if picker.filter.is_empty() {
                "(none)"
            } else {
                &picker.filter
            },
        ),
    ];
    if let Some(row) = selected_row {
        detail.push(Line::from(""));
        detail.push(line_kv("selected", &row.short_ref));
        detail.push(line_kv("ref", &row.item_ref));
        for (key, value) in row.summary.iter().take(8) {
            detail.push(line_kv(key, value));
        }
    }
    detail.push(Line::from(""));
    detail.push(Line::from(
        "↑/↓ choose | Enter continue | / filter | m manual | M toggle back | r refresh | Esc back",
    ));
    frame.render_widget(
        Paragraph::new(detail)
            .block(
                Block::default()
                    .title("Picker Detail")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn render_server_config(
    frame: &mut Frame<'_>,
    area: Rect,
    wizard: &RuntimeWizardState,
    title: &str,
) {
    let fields = [
        ("host", wizard.draft.host.as_str()),
        ("port", wizard.draft.port.as_str()),
        (
            "lazy_load",
            if wizard.draft.lazy_load {
                "true"
            } else {
                "false"
            },
        ),
        (
            "idle_seconds",
            if wizard.draft.idle_seconds.is_empty() {
                "(empty)"
            } else {
                &wizard.draft.idle_seconds
            },
        ),
    ];
    render_field_lines(
        frame,
        area,
        title,
        &fields,
        wizard.selected_field,
        "Space toggles booleans | Enter review | Esc back",
    );
}

fn render_backend_step(
    frame: &mut Frame<'_>,
    area: Rect,
    wizard: &RuntimeWizardState,
    title: &str,
) {
    let mut lines = vec![
        line_kv("model", short_ref(&wizard.draft.model_ref)),
        line_kv("dataset", short_ref(&wizard.draft.dataset_ref)),
        Line::from(""),
    ];
    for (index, backend) in RuntimeWizardBackend::ALL.iter().enumerate() {
        let selected = index == wizard.selected_field;
        let style = selected_style(selected);
        lines.push(Line::from(vec![
            Span::styled(if selected { "● " } else { "○ " }, style),
            Span::styled(backend.label(), style),
        ]));
    }
    if wizard.draft.backend == RuntimeWizardBackend::Manual {
        lines.push(line_kv("manual", &wizard.draft.manual_backend));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "↑/↓ choose | Enter continue | type edits manual | Esc back",
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_plan_basics(frame: &mut Frame<'_>, area: Rect, wizard: &RuntimeWizardState, title: &str) {
    let fields = [(
        "name",
        if wizard.draft.name.is_empty() {
            "(empty)"
        } else {
            &wizard.draft.name
        },
    )];
    render_field_lines(
        frame,
        area,
        title,
        &fields,
        wizard.selected_field,
        "Enter continue | Esc back",
    );
}

fn render_advanced_choice(
    frame: &mut Frame<'_>,
    area: Rect,
    wizard: &RuntimeWizardState,
    title: &str,
) {
    let choices = [
        RuntimeWizardAdvancedChoice::Defaults,
        RuntimeWizardAdvancedChoice::Customize,
    ];
    let mut lines = vec![
        line_kv("current", wizard.draft.advanced_choice.label()),
        Line::from(""),
    ];
    for (index, choice) in choices.iter().enumerate() {
        let selected = index == wizard.selected_field;
        let style = selected_style(selected);
        lines.push(Line::from(vec![
            Span::styled(if selected { "● " } else { "○ " }, style),
            Span::styled(choice.label(), style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("↑/↓ choose | Enter continue | Esc back"));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_advanced_fields(
    frame: &mut Frame<'_>,
    area: Rect,
    wizard: &RuntimeWizardState,
    title: &str,
) {
    let fields = advanced_field_values(wizard);
    let borrowed = fields
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect::<Vec<_>>();
    render_field_lines(
        frame,
        area,
        title,
        &borrowed,
        wizard.selected_field,
        "Space toggles tri-state bools | Enter review | Esc back",
    );
}

fn render_review(
    frame: &mut Frame<'_>,
    area: Rect,
    wizard: &RuntimeWizardState,
    title: &str,
    running: Option<String>,
) {
    let rows = wizard.review_rows();
    let chunks = if area.width > 110 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area)
    };
    let mut list = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let selected = index == wizard.selected_review_row;
        let style = selected_style(selected);
        let line = match row {
            RuntimeWizardReviewRow::Field(key, value) => Line::from(vec![
                Span::styled(if selected { "● " } else { "○ " }, style),
                Span::styled(format!("{key}: "), style),
                Span::raw(value.clone()),
            ]),
            RuntimeWizardReviewRow::Preview => Line::from(vec![
                Span::styled(if selected { "● " } else { "○ " }, style),
                Span::styled("Preview LoRA plan", style),
            ]),
            RuntimeWizardReviewRow::Submit => Line::from(vec![
                Span::styled(if selected { "● " } else { "○ " }, style),
                Span::styled("Submit", style),
            ]),
        };
        list.push(ListItem::new(line));
    }
    frame.render_widget(
        List::new(list).block(Block::default().title(title).borders(Borders::ALL)),
        chunks[0],
    );

    let mut detail = vec![
        line_kv("preview", preview_label(wizard)),
        line_kv(
            "dirty_since_preview",
            wizard.dirty_since_preview.to_string(),
        ),
    ];
    if let Some(running) = running {
        detail.push(line_kv("running", running));
    }
    if !wizard.validation_errors.is_empty() {
        detail.push(Line::from(""));
        for error in &wizard.validation_errors {
            detail.push(Line::from(vec![
                Span::styled("error: ", Style::default().fg(Color::Red)),
                Span::raw(error.clone()),
            ]));
        }
    }
    if let Some(message) = &wizard.preview.message {
        detail.push(Line::from(""));
        detail.push(line_kv("preview_message", message));
    }
    for (key, value) in wizard.preview.lines.iter().take(12) {
        detail.push(line_kv(key, value));
    }
    detail.push(Line::from(""));
    detail.push(Line::from(
        "Enter edit/preview/submit | r preview/refresh | Esc back",
    ));
    frame.render_widget(
        Paragraph::new(detail)
            .block(
                Block::default()
                    .title("Review / Preview")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn render_field_lines(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    fields: &[(&str, &str)],
    selected_index: usize,
    footer: &str,
) {
    let mut lines = Vec::new();
    for (index, (key, value)) in fields.iter().enumerate() {
        let selected = index == selected_index;
        let style = selected_style(selected);
        lines.push(Line::from(vec![
            Span::styled(if selected { "● " } else { "○ " }, style),
            Span::styled(format!("{key}: "), style),
            Span::raw(if selected && *value == "(empty)" {
                "▌".to_string()
            } else {
                value.to_string()
            }),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(footer.to_string()));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn picker_visible_rows<'a>(
    navigator: &'a NavigatorState,
    kind: NavigatorListKind,
    filter: &str,
) -> Vec<&'a NavigatorRow> {
    let state = navigator.state(kind);
    let filter = filter.trim().to_ascii_lowercase();
    if filter.is_empty() {
        return state.rows.iter().collect();
    }
    state
        .rows
        .iter()
        .filter(|row| row.search_text.to_ascii_lowercase().contains(&filter))
        .collect()
}

fn advanced_field_values(wizard: &RuntimeWizardState) -> Vec<(&'static str, String)> {
    vec![
        (
            "max_seq_length",
            empty_or_value(&wizard.draft.max_seq_length),
        ),
        ("rank", empty_or_value(&wizard.draft.rank)),
        ("learning_rate", empty_or_value(&wizard.draft.learning_rate)),
        ("batch_size", empty_or_value(&wizard.draft.batch_size)),
        (
            "gradient_accumulation_steps",
            empty_or_value(&wizard.draft.gradient_accumulation_steps),
        ),
        ("max_steps", empty_or_value(&wizard.draft.max_steps)),
        ("seed", empty_or_value(&wizard.draft.seed)),
        ("mask_prompt", option_bool_label(wizard.draft.mask_prompt)),
        (
            "mlx_num_layers",
            empty_or_value(&wizard.draft.mlx_num_layers),
        ),
        (
            "mlx_grad_checkpoint",
            option_bool_label(wizard.draft.mlx_grad_checkpoint),
        ),
        (
            "peft_load_in_4bit",
            option_bool_label(wizard.draft.peft_load_in_4bit),
        ),
        (
            "peft_load_in_8bit",
            option_bool_label(wizard.draft.peft_load_in_8bit),
        ),
    ]
}

fn preview_label(wizard: &RuntimeWizardState) -> String {
    match wizard.preview.status {
        RuntimePreviewStatus::NotRun => "not run".to_string(),
        RuntimePreviewStatus::Running => "running".to_string(),
        RuntimePreviewStatus::Ready => "ready".to_string(),
        RuntimePreviewStatus::Stale => "stale; re-preview required".to_string(),
        RuntimePreviewStatus::Blocked => "blocked".to_string(),
        RuntimePreviewStatus::Error => "error".to_string(),
    }
}

fn short_ref(value: &str) -> String {
    if value.len() > 12 {
        value.chars().take(12).collect()
    } else if value.is_empty() {
        "(empty)".to_string()
    } else {
        value.to_string()
    }
}

fn empty_or_value(value: &str) -> String {
    if value.trim().is_empty() {
        "(default)".to_string()
    } else {
        value.to_string()
    }
}

fn option_bool_label(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "(default)".to_string())
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
