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
    daemon_client::{token_source_label, url_source_label},
};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(3),
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
    match app.selected_menu_entry().item {
        MenuItem::StartDaemon => render_start_detail(frame, area, app),
        MenuItem::ProviderAuth => render_provider_detail(frame, area, app),
        MenuItem::Settings => render_settings_detail(frame, area, app),
        MenuItem::Dashboard => render_dashboard(frame, area, app),
        MenuItem::Models
        | MenuItem::Adapters
        | MenuItem::Datasets
        | MenuItem::Servers
        | MenuItem::Sessions
        | MenuItem::Training => render_slice2_placeholder(frame, area, app),
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
    lines.push(Line::from("Slice 2 will add read-only navigators for models, servers, sessions, datasets, and training."));

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

fn render_slice2_placeholder(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let entry = app.selected_menu_entry();
    let lines = vec![
        Line::from(vec![
            Span::styled(entry.label, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" is not implemented in Slice 1.1."),
        ]),
        Line::from(""),
        Line::from("This operator shell intentionally lands before read-only navigators."),
        Line::from("Use Dashboard and Settings in this slice."),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(entry.label).borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = Vec::new();
    if let Some(input) = app.active_input_line() {
        lines.push(render_input_line(input));
    } else {
        lines.push(Line::from(
            "↑/↓ move | Enter select/edit | Tab detail | Esc menu | r refresh | q quit | provider: k set key, x remove",
        ));
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
}
