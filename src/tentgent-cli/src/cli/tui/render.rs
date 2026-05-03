use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};
use serde_json::Value;

use super::{
    app::{InputMode, Screen, TuiApp},
    daemon_client::{token_source_label, url_source_label},
};

pub(super) fn render(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    let tabs = Tabs::new(vec!["Status", "Settings"])
        .select(app.screen.index())
        .block(Block::default().title("Tentgent TUI").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, chunks[0]);

    match app.screen {
        Screen::Status => render_status(frame, chunks[1], app),
        Screen::Settings => render_settings(frame, chunks[1], app),
    }

    let footer = footer_lines(app);
    frame.render_widget(
        Paragraph::new(footer)
            .block(Block::default().borders(Borders::ALL).title("Command"))
            .wrap(Wrap { trim: true }),
        chunks[2],
    );
}

fn render_status(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &TuiApp) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);

    let mut left = vec![
        line_kv("home", app.home.display().to_string()),
        line_kv("daemon_url", &app.daemon_url.url),
        line_kv("url_source", url_source_label(app.daemon_url.source)),
        line_kv("token_source", token_source_label(app.daemon_token.source)),
        line_kv("daemon_state", app.daemon.state.label()),
        line_kv("detail", &app.daemon.detail),
    ];
    if let Some(error) = &app.config_error {
        left.push(line_kv("config_error", error));
    }
    if !app.inspection.running {
        left.push(Line::from(""));
        left.push(Line::from("Daemon is not running."));
        left.push(line_kv("start", app.start_command()));
    }
    left.push(Line::from(""));
    left.push(line_kv(
        "runtime_dir",
        app.inspection.runtime_dir.display().to_string(),
    ));
    left.push(line_kv(
        "log_dir",
        app.inspection.log_dir.display().to_string(),
    ));
    left.push(line_kv(
        "stdout_log",
        app.inspection.stdout_log_path.display().to_string(),
    ));
    left.push(line_kv(
        "stderr_log",
        app.inspection.stderr_log_path.display().to_string(),
    ));

    frame.render_widget(
        Paragraph::new(left)
            .block(Block::default().title("Status").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        columns[0],
    );

    let mut right = Vec::new();
    if let Some(status) = &app.daemon.status {
        right.push(Line::from(Span::styled(
            "Daemon HTTP",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        push_json_kv(&mut right, status, "status");
        push_json_kv(&mut right, status, "pid");
        push_json_kv(&mut right, status, "host");
        push_json_kv(&mut right, status, "port");
        push_json_kv(&mut right, status, "runtime_home");
        push_json_kv(&mut right, status, "process_path");
        push_json_kv(&mut right, status, "pid_path");
    } else {
        right.push(Line::from("Daemon HTTP data unavailable."));
    }
    right.push(Line::from(""));
    right.extend(auth_summary_lines(app.daemon.auth.as_ref(), &app.auth_rows));
    right.push(Line::from(""));
    right.extend(doctor_summary_lines(app.daemon.doctor.as_ref()));

    frame.render_widget(
        Paragraph::new(right)
            .block(Block::default().title("Live Data").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        columns[1],
    );
}

fn render_settings(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &TuiApp) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let daemon_url_pref = app.config.daemon.url.as_deref().unwrap_or("(unset)");
    let mut config_lines = vec![
        line_kv("config", app.config_path.display().to_string()),
        line_kv("schema_version", app.config.schema_version.to_string()),
        line_kv("last_section", &app.config.tui.last_section),
        line_kv(
            "auto_start_daemon",
            app.config.tui.auto_start_daemon.to_string(),
        ),
        line_kv("daemon.url", daemon_url_pref),
        line_kv("resolved_url", &app.daemon_url.url),
        line_kv("url_source", url_source_label(app.daemon_url.source)),
        line_kv("token_source", token_source_label(app.daemon_token.source)),
    ];
    if let Some(error) = &app.config_error {
        config_lines.push(line_kv("config_error", error));
    }

    frame.render_widget(
        Paragraph::new(config_lines)
            .block(Block::default().title("Config").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        columns[0],
    );

    let provider_items: Vec<ListItem> = app
        .auth_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let marker = if index == app.selected_provider {
                ">"
            } else {
                " "
            };
            let source = row.effective_source.unwrap_or("none");
            let line = format!(
                "{marker} {}  source={} env={} keychain={}  {}",
                row.provider.display_name(),
                source,
                row.env_present,
                row.keychain_present,
                row.note
            );
            ListItem::new(line)
        })
        .collect();
    frame.render_widget(
        List::new(provider_items).block(
            Block::default()
                .title("Provider Auth")
                .borders(Borders::ALL),
        ),
        columns[1],
    );
}

fn footer_lines(app: &TuiApp) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let input = app.active_input_label();
    match input {
        Some(value) => lines.push(Line::from(value)),
        None => lines.push(Line::from(
            "q quit | Ctrl-C quit | r refresh | s start daemon | tab switch | u edit URL | k set key | x remove key",
        )),
    }
    if !app.message.is_empty() {
        lines.push(Line::from(app.message.clone()));
    }
    if matches!(app.input_mode, InputMode::Normal) {
        lines.push(Line::from(
            "1 status | 2 settings | up/down select provider",
        ));
    }
    lines
}

fn auth_summary_lines(
    daemon_auth: Option<&Value>,
    local_rows: &[super::app::ProviderAuthRow],
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Auth",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    if let Some(value) = daemon_auth {
        let count = value
            .get("providers")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        lines.push(line_kv("daemon_auth_providers", count.to_string()));
    } else {
        lines.push(Line::from("daemon auth unavailable; showing local state"));
    }
    for row in local_rows {
        let source = row.effective_source.unwrap_or("none");
        lines.push(Line::from(format!(
            "{}: source={} {}",
            row.provider.display_name(),
            source,
            row.note
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
