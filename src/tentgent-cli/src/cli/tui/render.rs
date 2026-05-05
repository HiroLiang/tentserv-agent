use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame,
};
use serde_json::Value;

use super::{
    app::{
        AppMode, BootstrapReason, DaemonActionState, FocusPane, InputLine, MenuItem, StartPhase,
        TuiApp,
    },
    chat_render::render_chat,
    daemon_client::{token_source_label, url_source_label},
    jobs::{job_progress_label, sanitize_job_summary, JobLoadState, TuiJobItem},
    navigator::{DashboardCard, NavigatorListKind, NavigatorLoadState},
    resource_render::{render_resources, resource_summary_lines},
    runtime_action_render::render_runtime_action,
    session_action::SessionActionState,
    store_action_render::render_store_action,
};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(area);

    render_header(frame, chunks[0], app);
    render_body(frame, chunks[1], app);
    render_footer(frame, chunks[2], app);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mode_style = match app.mode {
        AppMode::Operator => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        AppMode::Bootstrap(BootstrapReason::AuthRequired) => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        AppMode::Bootstrap(_) => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("tentgent", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" · "),
            Span::styled(app.mode.label(), mode_style),
            Span::raw(" · "),
            Span::raw(app.daemon.state.label()),
        ]),
        Line::from(vec![
            kv_span("home", app.home.display().to_string()),
            Span::raw("  "),
            kv_span("url", app.daemon_url.url.clone()),
            Span::raw("  "),
            kv_span("token", token_source_label(app.daemon_token.source)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Tentgent TUI").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_body(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if area.width < 100 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(10), Constraint::Min(8)])
            .split(area);
        render_menu(frame, rows[0], app);
        render_detail(frame, rows[1], app);
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(34), Constraint::Min(40)])
            .split(area);
        render_menu(frame, columns[0], app);
        render_detail(frame, columns[1], app);
    }
}

fn render_menu(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let entries = app.menu_entries();
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let selected = index == app.selected_menu && app.focus == FocusPane::Menu;
            let marker = if index == app.selected_menu {
                "●"
            } else {
                "○"
            };
            let mut style = if entry.enabled {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            if selected {
                style = style.fg(Color::Cyan).add_modifier(Modifier::BOLD);
            }
            ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::raw(" "),
                Span::styled(entry.label, style),
                Span::raw("  "),
                Span::styled(entry.detail, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(Block::default().title("Menu").borders(Borders::ALL)),
        area,
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if app.session_action.is_active() {
        render_session_action(frame, area, &app.session_action);
        return;
    }
    match app.selected_menu_entry().item {
        MenuItem::StartDaemon => render_start_detail(frame, area, app),
        MenuItem::ProviderAuth => render_provider_detail(frame, area, app),
        MenuItem::Settings => render_settings_detail(frame, area, app),
        MenuItem::Dashboard => render_dashboard(frame, area, app),
        MenuItem::Chat => render_chat(frame, area, app),
        MenuItem::Jobs => render_jobs(frame, area, app),
        MenuItem::Models
        | MenuItem::Adapters
        | MenuItem::Datasets
        | MenuItem::Servers
        | MenuItem::Sessions
        | MenuItem::Training => render_navigator(frame, area, app),
        MenuItem::Resources => render_resources(frame, area, app),
    }
}

fn render_start_detail(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = vec![
        line_kv("state", app.daemon.state.label()),
        line_kv("detail", &app.daemon.detail),
        line_kv("resolved_url", &app.daemon_url.url),
        line_kv("url_source", url_source_label(app.daemon_url.source)),
        line_kv("start", app.start_command()),
        line_kv(
            "stdout_log",
            app.inspection.stdout_log_path.display().to_string(),
        ),
        line_kv(
            "stderr_log",
            app.inspection.stderr_log_path.display().to_string(),
        ),
    ];
    if let Some(error) = &app.config_error {
        lines.push(line_kv("config_error", error));
    }
    if let Some(warning) = app.start_target_warning() {
        lines.push(line_kv("warning", warning));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Progress",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.extend(start_progress_lines(app));

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Start Daemon").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn start_progress_lines(app: &TuiApp) -> Vec<Line<'static>> {
    match &app.daemon_action {
        DaemonActionState::Idle => vec![Line::from("○ waiting for explicit start action")],
        DaemonActionState::Starting { phase, warning, .. } => {
            let mut lines = Vec::new();
            lines.push(phase_line(
                "resolving home",
                *phase,
                StartPhase::ResolvingHome,
            ));
            lines.push(phase_line(
                "spawning detached daemon",
                *phase,
                StartPhase::SpawningDetachedDaemon,
            ));
            lines.push(phase_line(
                "polling /healthz",
                *phase,
                StartPhase::PollingHealthz,
            ));
            if let Some(warning) = warning {
                lines.push(line_kv("warning", warning));
            }
            lines
        }
        DaemonActionState::StartFailed {
            message,
            stdout_log,
            stderr_log,
        } => {
            let mut lines = vec![Line::from(vec![
                Span::styled("● failed: ", Style::default().fg(Color::Red)),
                Span::raw(message.clone()),
            ])];
            if let Some(path) = stdout_log {
                lines.push(line_kv("stdout_log", path.display().to_string()));
            }
            if let Some(path) = stderr_log {
                lines.push(line_kv("stderr_log", path.display().to_string()));
            }
            lines
        }
        DaemonActionState::Ready => vec![Line::from(vec![
            Span::styled("● ready", Style::default().fg(Color::Green)),
            Span::raw("; switching to operator mode"),
        ])],
    }
}

fn phase_line(label: &'static str, current: StartPhase, phase: StartPhase) -> Line<'static> {
    let marker = if current == phase {
        "◐"
    } else if phase_rank(current) > phase_rank(phase) {
        "●"
    } else {
        "○"
    };
    Line::from(format!("{marker} {label}"))
}

fn phase_rank(phase: StartPhase) -> u8 {
    match phase {
        StartPhase::ResolvingHome => 0,
        StartPhase::SpawningDetachedDaemon => 1,
        StartPhase::PollingHealthz => 2,
    }
}

fn render_provider_detail(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let rows = app.auth_rows.iter().enumerate().map(|(index, row)| {
        let selected = index == app.selected_provider;
        let marker = if selected && app.focus == FocusPane::Detail {
            "●"
        } else if selected {
            ">"
        } else {
            "○"
        };
        let style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new(vec![
            Cell::from(marker),
            Cell::from(row.provider.display_name()),
            Cell::from(row.provider.env_var()),
            Cell::from(row.state.source_label()),
            Cell::from(row.state.label()),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Length(18),
            Constraint::Length(22),
            Constraint::Min(22),
        ],
    )
    .header(
        Row::new(vec!["", "Provider", "Env", "Source", "Status"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .title("Provider Auth")
            .borders(Borders::ALL),
    );
    frame.render_widget(table, area);
}

fn render_settings_detail(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let table_rows = app
        .settings_entries()
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let selected = index == app.selected_setting;
            let marker = if selected && app.focus == FocusPane::Detail {
                "●"
            } else if selected {
                ">"
            } else {
                "○"
            };
            let style = if selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let edit = if entry.editable { "edit" } else { "read-only" };
            Row::new(vec![
                marker.to_string(),
                entry.label.to_string(),
                entry.value,
                entry.detail.to_string(),
                edit.to_string(),
            ])
            .style(style)
        });
    let table = Table::new(
        table_rows,
        [
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Length(28),
            Constraint::Min(24),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec!["", "Setting", "Value", "Applies", "Mode"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().title("Settings").borders(Borders::ALL));
    frame.render_widget(table, area);
}

fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Daemon",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        line_kv("state", app.daemon.state.label()),
        line_kv("detail", &app.daemon.detail),
    ];
    if let Some(status) = &app.daemon.status {
        push_json_kv(&mut lines, status, "status");
        push_json_kv(&mut lines, status, "pid");
        push_json_kv(&mut lines, status, "host");
        push_json_kv(&mut lines, status, "port");
        push_json_kv(&mut lines, status, "runtime_home");
    }
    lines.push(Line::from(""));
    lines.extend(auth_summary_lines(app));
    lines.push(Line::from(""));
    lines.extend(doctor_summary_lines(app.daemon.doctor.as_ref()));
    lines.push(Line::from(""));
    lines.extend(job_summary_lines(app));
    lines.push(Line::from(""));
    lines.extend(dashboard_count_lines(app));
    lines.push(Line::from(""));
    lines.extend(resource_summary_lines(app));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Operator Dashboard")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_jobs(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(8)])
        .split(area);
    let compact = area.width < 110;
    let rows = app.jobs.jobs.iter().enumerate().map(|(index, job)| {
        let selected = index == app.jobs.selected && app.focus == FocusPane::Detail;
        let marker = if index == app.jobs.selected {
            if selected {
                "●"
            } else {
                ">"
            }
        } else {
            "○"
        };
        let style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        if compact {
            Row::new(vec![
                Cell::from(marker),
                Cell::from(job.status.clone()),
                Cell::from(job.label.clone()),
                Cell::from(job_progress_label(job)),
            ])
            .style(style)
        } else {
            Row::new(vec![
                Cell::from(marker),
                Cell::from(job.status.clone()),
                Cell::from(job.kind.clone()),
                Cell::from(job.label.clone()),
                Cell::from(job.stage.clone()),
                Cell::from(job_progress_label(job)),
                Cell::from(job.updated_at.clone()),
            ])
            .style(style)
        }
    });
    let (headers, widths) = if compact {
        (
            vec!["", "Status", "Label", "Progress"],
            vec![
                Constraint::Length(2),
                Constraint::Length(12),
                Constraint::Min(24),
                Constraint::Length(18),
            ],
        )
    } else {
        (
            vec![
                "", "Status", "Kind", "Label", "Stage", "Progress", "Updated",
            ],
            vec![
                Constraint::Length(2),
                Constraint::Length(12),
                Constraint::Length(16),
                Constraint::Min(22),
                Constraint::Length(22),
                Constraint::Length(18),
                Constraint::Length(24),
            ],
        )
    };
    let title = format!(
        "Jobs · {} total · {} active · {}",
        app.jobs.jobs.len(),
        app.jobs.active_jobs().len(),
        job_load_label(&app.jobs.load_state)
    );
    frame.render_widget(
        Table::new(rows, widths)
            .header(Row::new(headers).style(Style::default().add_modifier(Modifier::BOLD)))
            .block(Block::default().title(title).borders(Borders::ALL)),
        chunks[0],
    );

    let mut lines = Vec::new();
    if let Some(job) = app.jobs.jobs.get(app.jobs.selected) {
        lines.extend(job_detail_lines(job));
    } else {
        lines.push(Line::from(
            "No background jobs yet. Long pull/import/synth/eval actions appear here.",
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "r refresh | b background | Esc menu | no cancel in Slice 4.1",
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Job Detail").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn render_session_action(frame: &mut Frame<'_>, area: Rect, state: &SessionActionState) {
    let lines = match state {
        SessionActionState::Idle => vec![Line::from("No session action active.")],
        SessionActionState::ConfirmingDelete {
            target,
            typed,
            message,
            ..
        } => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Delete session",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(message.clone()),
                Line::from(""),
                line_kv("short_ref", &target.short_ref),
                line_kv("full_ref", &target.session_ref),
                line_kv("title", &target.title),
            ];
            if target.require_full_ref {
                lines.push(line_kv(
                    "warning",
                    "short ref is ambiguous; full ref is required",
                ));
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
                "Enter confirms when text matches; Esc cancels and preserves selection.",
            ));
            lines
        }
        SessionActionState::Running {
            request,
            started_at,
            ..
        } => {
            let elapsed = started_at.elapsed().as_secs();
            vec![
                line_kv("action", "Delete session"),
                line_kv("state", "waiting for daemon response"),
                line_kv("short_ref", &request.target.short_ref),
                line_kv("full_ref", &request.target.session_ref),
                line_kv("elapsed", format!("{elapsed}s")),
                Line::from(""),
                Line::from(
                    "Esc aborts only the local TUI wait; daemon-side delete may have happened.",
                ),
            ]
        }
        SessionActionState::Result(result) => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Session deleted",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                line_kv("status", result.status.to_string()),
            ];
            for (key, value) in &result.lines {
                lines.push(line_kv(key, value));
            }
            lines.push(line_kv("summary", &result.raw_summary));
            lines.push(Line::from(""));
            lines.push(Line::from("Enter/Esc returns to the previous view."));
            lines
        }
        SessionActionState::Error {
            target,
            message,
            recoverable,
        } => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Session action error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                line_kv("error", message),
                line_kv("recoverable", recoverable.to_string()),
            ];
            if let Some(target) = target {
                lines.push(line_kv("short_ref", &target.short_ref));
                lines.push(line_kv("full_ref", &target.session_ref));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("Enter/Esc returns to the previous view."));
            lines
        }
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Session Action")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_navigator(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let Some(kind) = app.current_navigator_kind() else {
        return;
    };
    if app.store_action.is_active() {
        render_store_action(frame, area, &app.store_action);
        return;
    }
    if app.runtime_action.is_active() {
        render_runtime_action(frame, area, &app.runtime_action, &app.navigator);
        return;
    }
    let state = app.navigator.state(kind);
    if let Some(tail) = &state.active_tail {
        let mut lines = vec![
            line_kv("source", tail.source.title()),
            line_kv("loaded_at", &tail.loaded_at),
            line_kv("truncated", tail.truncated.to_string()),
            line_kv("scroll", tail.scroll_offset.to_string()),
        ];
        if let Some(error) = &tail.error {
            lines.push(line_kv("error", error));
        }
        lines.push(Line::from(""));
        for line in tail
            .lines
            .iter()
            .take(area.height.saturating_sub(8) as usize)
        {
            lines.push(Line::from(line.clone()));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .title(tail.source.title())
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let rows = state.visible_rows();
    let chunks = if area.height > 18 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(9)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(5)])
            .split(area)
    };
    render_navigator_table(frame, chunks[0], app, kind);
    render_navigator_detail(frame, chunks[1], app, kind, rows.len());
}

fn render_navigator_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    kind: NavigatorListKind,
) {
    let state = app.navigator.state(kind);
    let visible = state.visible_rows();
    let headers = kind.column_headers();
    let compact = area.width < 110;
    let rows = visible.iter().enumerate().map(|(index, row)| {
        let selected = index == state.selected_index && app.focus == FocusPane::Detail;
        let marker = if index == state.selected_index {
            if selected {
                "●"
            } else {
                ">"
            }
        } else {
            "○"
        };
        let style = if selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let mut cells = vec![Cell::from(marker.to_string())];
        let take = if compact { 3 } else { 5 };
        for value in row.columns.iter().take(take) {
            cells.push(Cell::from(value.clone()));
        }
        Row::new(cells).style(style)
    });
    let mut header_cells = vec![Cell::from("")];
    let take = if compact { 3 } else { 5 };
    header_cells.extend(headers.iter().take(take).map(|value| Cell::from(*value)));
    let widths = if compact {
        vec![
            Constraint::Length(2),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Min(18),
        ]
    } else {
        vec![
            Constraint::Length(2),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(18),
            Constraint::Length(18),
            Constraint::Min(20),
        ]
    };
    let title = if kind == NavigatorListKind::TrainPlans || kind == NavigatorListKind::TrainRuns {
        format!(
            "{} · {} tab · filter `{}` · {} rows · {}",
            kind.title(),
            app.navigator.training_tab.label(),
            state.filter,
            visible.len(),
            state.load_state.label()
        )
    } else {
        format!(
            "{} · filter `{}` · {} rows · {}",
            kind.title(),
            state.filter,
            visible.len(),
            state.load_state.label()
        )
    };
    frame.render_widget(
        Table::new(rows, widths)
            .header(Row::new(header_cells).style(Style::default().add_modifier(Modifier::BOLD)))
            .block(Block::default().title(title).borders(Borders::ALL)),
        area,
    );
}

fn render_navigator_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    kind: NavigatorListKind,
    visible_count: usize,
) {
    let state = app.navigator.state(kind);
    let mut lines = Vec::new();
    if visible_count == 0 {
        lines.push(Line::from(match &state.load_state {
            NavigatorLoadState::Idle => "not loaded; press r to refresh",
            NavigatorLoadState::Loading { .. } => "loading rows",
            NavigatorLoadState::Ready => "empty",
            NavigatorLoadState::Error { .. } | NavigatorLoadState::StaleItem { .. } => {
                "no visible rows; see state above"
            }
        }));
    } else if let Some(row) = state.selected_row() {
        lines.push(Line::from(vec![
            Span::styled("Selected ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&row.item_ref),
        ]));
        lines.push(line_kv("short_ref", row.short_ref.as_str()));
        if let Some(detail) = state.selected_detail() {
            lines.push(line_kv("detail_ref", detail.item_ref.as_str()));
            lines.push(line_kv("detail_loaded_at", detail.loaded_at.as_str()));
            lines.push(line_kv(
                "raw_fields",
                detail
                    .raw
                    .as_object()
                    .map(|object| object.len().to_string())
                    .unwrap_or_else(|| "0".to_string()),
            ));
            for (key, value) in detail.lines.iter().take(6) {
                lines.push(line_kv(key.as_str(), value.as_str()));
            }
        } else {
            for (key, value) in row.summary.iter().take(7) {
                lines.push(line_kv(key.as_str(), value.as_str()));
            }
        }
    }
    lines.push(Line::from(""));
    lines.extend(navigator_action_lines(kind, app));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Detail").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn navigator_action_lines(kind: NavigatorListKind, app: &TuiApp) -> Vec<Line<'static>> {
    let mut actions = if matches!(
        kind,
        NavigatorListKind::Models | NavigatorListKind::Adapters | NavigatorListKind::Datasets
    ) {
        let shortcut = if matches!(
            kind,
            NavigatorListKind::Models | NavigatorListKind::Datasets
        ) {
            " | A runtime shortcut"
        } else {
            ""
        };
        vec![Line::from(format!(
            "a actions{shortcut} | Enter inspect | / filter | r refresh | Esc menu"
        ))]
    } else if matches!(
        kind,
        NavigatorListKind::Servers | NavigatorListKind::TrainPlans
    ) {
        vec![Line::from(
            "a runtime actions | Enter inspect | / filter | r refresh | Esc menu",
        )]
    } else {
        vec![Line::from(
            "Enter inspect | / filter | r refresh | Esc menu | read-only",
        )]
    };
    match kind {
        NavigatorListKind::Servers => actions.push(Line::from(format!(
            "l load {} log | o toggle stdout/stderr",
            app.navigator.state(kind).server_log_kind.label()
        ))),
        NavigatorListKind::Sessions => actions.push(Line::from("m load message tail")),
        NavigatorListKind::TrainPlans => actions.push(Line::from("Tab switch Plans/Runs")),
        NavigatorListKind::TrainRuns => {
            actions.push(Line::from(
                "l load raw log | p load metrics | Tab switch Plans/Runs",
            ));
        }
        _ => {}
    }
    actions
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = Vec::new();
    if let Some(input) = app.active_input_line() {
        lines.push(render_input_line(input));
    } else {
        lines.push(Line::from(command_hint_text(app)));
    }
    if let Some(job) = app.jobs.active_jobs().first() {
        lines.push(Line::from(vec![
            Span::styled("job: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "{} · {} · {}",
                job.label,
                job.status,
                job_progress_label(job)
            )),
        ]));
    }
    if !app.message.is_empty() {
        lines.push(Line::from(app.message.clone()));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Command").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn command_hint_text(app: &TuiApp) -> String {
    match app.selected_menu_entry().item {
        MenuItem::Chat => {
            if app.chat.phase == super::chat::ChatPhase::ChooseSession
                && app.can_delete_selected_chat_session()
            {
                "↑/↓ session | Enter select | n new | s server | a adapter | x delete session | h context | r refresh | Esc back | q quit".to_string()
            } else {
                "↑/↓ move | Enter select/send | n new | s server | a adapter | h context | r refresh | Esc back | q quit".to_string()
            }
        }
        MenuItem::Sessions => {
            "↑/↓ move | Enter inspect | x delete session | a actions | c chat | m messages | / filter | r refresh | Esc back | q quit".to_string()
        }
        MenuItem::Models | MenuItem::Datasets => {
            "↑/↓ move | Enter inspect | a actions | A shortcut | / filter | r refresh | Esc back | q quit".to_string()
        }
        MenuItem::Adapters => {
            "↑/↓ move | Enter inspect | a actions | / filter | r refresh | Esc back | q quit".to_string()
        }
        MenuItem::Servers | MenuItem::Training => {
            "↑/↓ move | Enter inspect | a actions | / filter | r refresh | l logs | p metrics | Esc back | q quit".to_string()
        }
        MenuItem::Jobs => "↑/↓ move | r refresh | b background | Esc back | q quit".to_string(),
        MenuItem::Resources => {
            "↑/↓ move | Tab resource tab | r scan | / filter | Esc back | q quit".to_string()
        }
        MenuItem::Settings => "↑/↓ move | Enter edit | Esc back | q quit".to_string(),
        MenuItem::ProviderAuth => {
            "↑/↓ move | Enter/check | k set key | x remove key | Esc back | q quit".to_string()
        }
        MenuItem::Dashboard => "↑/↓ move | r refresh | q quit".to_string(),
        MenuItem::StartDaemon => "s start daemon | r refresh | Esc back | q quit".to_string(),
    }
}

fn render_input_line(input: InputLine) -> Line<'static> {
    let display_value = if input.masked {
        mask_for_display(&input.value)
    } else {
        input.value
    };
    let chars: Vec<char> = display_value.chars().collect();
    let cursor = input.cursor.min(chars.len());
    let mut spans = vec![
        Span::styled(
            format!("{}: ", input.label),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(""),
    ];
    for (index, ch) in chars.iter().enumerate() {
        if index == cursor {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().bg(Color::Cyan).fg(Color::Black),
            ));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
    }
    if cursor == chars.len() {
        spans.push(Span::styled(
            " ",
            Style::default().bg(Color::Cyan).fg(Color::Black),
        ));
    }
    spans.push(Span::styled(
        "  ←/→ move",
        Style::default().fg(Color::DarkGray),
    ));
    Line::from(spans)
}

fn mask_for_display(value: &str) -> String {
    "*".repeat(value.chars().count())
}

fn auth_summary_lines(app: &TuiApp) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Auth",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(
        "source: env-only / manual checks; automatic refresh skips /v1/auth",
    ));
    for row in &app.auth_rows {
        lines.push(Line::from(format!(
            "{}: {}",
            row.provider.display_name(),
            row.state.label()
        )));
    }
    lines
}

fn dashboard_count_lines(app: &TuiApp) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Inventory",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for kind in [
        NavigatorListKind::Models,
        NavigatorListKind::Adapters,
        NavigatorListKind::Datasets,
        NavigatorListKind::Servers,
        NavigatorListKind::Sessions,
        NavigatorListKind::TrainPlans,
        NavigatorListKind::TrainRuns,
    ] {
        lines.push(dashboard_card_line(app.dashboard.card(kind)));
    }
    lines
}

fn job_summary_lines(app: &TuiApp) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Jobs",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    let active = app.jobs.active_jobs();
    if active.is_empty() {
        lines.push(Line::from(format!(
            "active: 0; recent: {}; {}",
            app.jobs.jobs.len(),
            job_load_label(&app.jobs.load_state)
        )));
    } else {
        for job in active.into_iter().take(3) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}: ", job.label),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(format!("{} · {}", job.status, job_progress_label(job))),
            ]));
        }
    }
    lines
}

fn job_detail_lines(job: &TuiJobItem) -> Vec<Line<'static>> {
    let mut lines = vec![
        line_kv("job_id", &job.job_id),
        line_kv("kind", &job.kind),
        line_kv("status", &job.status),
        line_kv("stage", &job.stage),
        line_kv("progress", job_progress_label(job)),
        line_kv("updated_at", &job.updated_at),
        line_kv("cancellable", job.cancellable.to_string()),
    ];
    if let Some(ref_value) = &job.target_ref {
        lines.push(line_kv("target_ref", ref_value));
    }
    if let Some(path) = &job.artifact_path {
        lines.push(line_kv("artifact_path", path));
    }
    lines.push(line_kv("summary", sanitize_job_summary(job)));
    lines
}

fn job_load_label(state: &JobLoadState) -> &'static str {
    match state {
        JobLoadState::Idle => "idle",
        JobLoadState::Loading { .. } => "loading",
        JobLoadState::Ready => "ready",
        JobLoadState::Error { stale, .. } if *stale => "stale error",
        JobLoadState::Error { .. } => "error",
    }
}

fn dashboard_card_line(card: DashboardCard) -> Line<'static> {
    let count = card.count_label.unwrap_or_else(|| "--".to_string());
    let stale = if card.stale { " stale" } else { "" };
    let updated = card
        .last_ok
        .map(|value| format!(" ok@{value}"))
        .unwrap_or_default();
    let error = card
        .error
        .map(|value| format!("; error: {value}"))
        .unwrap_or_default();
    Line::from(vec![
        Span::styled(
            format!("{}: ", card.label),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(format!("{count}{stale}{updated}{error}")),
    ])
}

fn doctor_summary_lines(doctor: Option<&Value>) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Doctor",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    let Some(doctor) = doctor else {
        lines.push(Line::from("doctor unavailable"));
        return lines;
    };
    push_json_kv(&mut lines, doctor, "status");
    if let Some(summary) = doctor.get("summary") {
        for key in ["pass", "warn", "fail", "skipped"] {
            push_json_kv(&mut lines, summary, key);
        }
    }
    lines
}

fn push_json_kv(lines: &mut Vec<Line<'static>>, value: &Value, key: &str) {
    if let Some(value) = value.get(key) {
        lines.push(line_kv(key, scalar_string(value)));
    }
}

fn scalar_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
        _ => value.to_string(),
    }
}

fn line_kv(key: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{}: ", key.into()),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(value.into()),
    ])
}

fn kv_span(key: &'static str, value: impl Into<String>) -> Span<'static> {
    Span::raw(format!("{key}: {}", value.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn compact_terminal_layout_renders_without_panic() {
        let home = std::env::temp_dir().join("tentgent-tui-render-compact");
        let app = TuiApp::test_app(home);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| render(frame, &app))
            .expect("compact render");
    }

    #[test]
    fn compact_chat_layout_renders_without_panic() {
        let home = std::env::temp_dir().join("tentgent-tui-render-chat");
        let mut app = TuiApp::test_app(home);
        app.daemon = crate::cli::tui::daemon_client::DaemonSnapshot {
            state: crate::cli::tui::daemon_client::DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.mode = AppMode::Operator;
        app.selected_menu = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Chat)
            .expect("chat menu");
        app.focus = FocusPane::Detail;
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| render(frame, &app))
            .expect("compact chat render");
    }

    #[test]
    fn compact_store_action_layout_renders_without_panic() {
        let home = std::env::temp_dir().join("tentgent-tui-render-action");
        let mut app = TuiApp::test_app(home);
        app.daemon = crate::cli::tui::daemon_client::DaemonSnapshot {
            state: crate::cli::tui::daemon_client::DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.mode = AppMode::Operator;
        app.selected_menu = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Models)
            .expect("models menu");
        app.focus = FocusPane::Detail;
        app.store_action = crate::cli::tui::store_action::ActionState::SelectingAction {
            kind: NavigatorListKind::Models,
            actions: crate::cli::tui::store_action::actions_for(NavigatorListKind::Models),
            selected: 0,
        };
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| render(frame, &app))
            .expect("compact action render");
    }

    #[test]
    fn compact_runtime_action_layout_renders_without_panic() {
        let home = std::env::temp_dir().join("tentgent-tui-render-runtime-action");
        let mut app = TuiApp::test_app(home);
        app.daemon = crate::cli::tui::daemon_client::DaemonSnapshot {
            state: crate::cli::tui::daemon_client::DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.mode = AppMode::Operator;
        app.selected_menu = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Servers)
            .expect("servers menu");
        app.focus = FocusPane::Detail;
        app.runtime_action = crate::cli::tui::runtime_action::RuntimeActionState::SelectingAction {
            kind: NavigatorListKind::Servers,
            actions: crate::cli::tui::runtime_action::runtime_actions_for(
                NavigatorListKind::Servers,
                NavigatorListKind::TrainPlans,
            ),
            selected: 0,
        };
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| render(frame, &app))
            .expect("compact runtime action render");
    }

    #[test]
    fn compact_session_action_layout_renders_without_panic() {
        let home = std::env::temp_dir().join("tentgent-tui-render-session-action");
        let mut app = TuiApp::test_app(home);
        app.mode = AppMode::Operator;
        app.selected_menu = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Sessions)
            .expect("sessions menu");
        app.focus = FocusPane::Detail;
        let target = crate::cli::tui::session_action::make_delete_target(
            "session-full-ref",
            "session-full",
            "test",
            crate::cli::tui::session_action::SessionActionOrigin::Navigator,
            vec!["session-full".to_string()],
        );
        app.session_action =
            crate::cli::tui::session_action::SessionActionState::ConfirmingDelete {
                message: target.confirmation_hint(),
                target,
                typed: String::new(),
                cursor: 0,
            };
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| render(frame, &app))
            .expect("compact session action render");
    }

    #[test]
    fn footer_hints_are_focus_specific_for_chat_and_sessions() {
        let home = std::env::temp_dir().join("tentgent-tui-render-footer");
        let mut app = TuiApp::test_app(home);
        app.daemon = crate::cli::tui::daemon_client::DaemonSnapshot {
            state: crate::cli::tui::daemon_client::DaemonConnectionState::Ready,
            detail: "ready".to_string(),
            status: None,
            doctor: None,
        };
        app.mode = AppMode::Operator;
        app.selected_menu = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Chat)
            .expect("chat menu");
        app.focus = FocusPane::Detail;
        app.chat.phase = crate::cli::tui::chat::ChatPhase::ChooseSession;
        app.chat.focus = crate::cli::tui::chat::ChatFocus::Chooser;
        app.chat.selected_session = 1;
        app.chat.sessions = vec![crate::cli::tui::chat::ChatSessionRow {
            session_ref: "session-full-ref".to_string(),
            short_ref: "session-full".to_string(),
            title: "test".to_string(),
            message_count: Some(1),
            updated_at: None,
            default_server_ref: None,
            adapter_ref: None,
            raw: serde_json::Value::Null,
        }];

        let chat_hint = command_hint_text(&app);
        assert!(chat_hint.contains("a adapter"));
        assert!(chat_hint.contains("x delete session"));
        assert!(!chat_hint.contains("a actions"));

        app.selected_menu = app
            .menu_entries()
            .iter()
            .position(|entry| entry.item == MenuItem::Sessions)
            .expect("sessions menu");

        let sessions_hint = command_hint_text(&app);
        assert!(sessions_hint.contains("a actions"));
        assert!(sessions_hint.contains("x delete session"));
        assert!(!sessions_hint.contains("a adapter"));
    }
}
