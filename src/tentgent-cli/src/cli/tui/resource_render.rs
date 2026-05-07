use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    Frame,
};

use super::super::display::format_bytes;
use super::{
    app::TuiApp,
    resource::{DiskState, ResourceLoadState, ResourceTab},
};

pub(super) fn render_resources(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let chunks = if area.height > 16 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(8)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(6)])
            .split(area)
    };
    render_resource_header(frame, chunks[0], app);
    match app.resources.tab {
        ResourceTab::Storage => render_resource_storage(frame, chunks[1], app),
        ResourceTab::Processes => render_resource_processes(frame, chunks[1], app),
        ResourceTab::Warnings => render_resource_warnings(frame, chunks[1], app),
    }
}

pub(super) fn resource_summary_lines(app: &TuiApp) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Resources",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    let Some(snapshot) = &app.resources.snapshot else {
        lines.push(Line::from(
            "not scanned; open Resources and press r for local disk/process monitor",
        ));
        return lines;
    };
    lines.push(line_kv(
        "storage",
        format!(
            "{} across {} categories",
            format_bytes(snapshot.storage_total_bytes()),
            snapshot.storage_rows.len()
        ),
    ));
    lines.push(line_kv(
        "disk_free",
        snapshot
            .disk
            .available_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "unknown".to_string()),
    ));
    lines.push(line_kv(
        "processes",
        snapshot.process_rows.len().to_string(),
    ));
    lines.push(line_kv("warnings", snapshot.warnings.len().to_string()));
    if snapshot.partial {
        lines.push(Line::from(
            "resource scan is partial; open Resources for details",
        ));
    }
    lines
}

fn render_resource_header(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let mut lines = vec![Line::from(vec![
        kv_span("tab", app.resources.tab.label()),
        Span::raw("  "),
        kv_span("state", app.resources.load_state.label()),
        Span::raw("  "),
        kv_span("filter", format!("`{}`", app.resources.filter)),
    ])];
    if let Some(snapshot) = &app.resources.snapshot {
        lines.push(Line::from(vec![
            kv_span("last", snapshot.last_refreshed.clone()),
            Span::raw("  "),
            kv_span("duration", format!("{}ms", snapshot.scan_duration_ms)),
            Span::raw("  "),
            kv_span("entries", snapshot.scanned_files.to_string()),
            Span::raw("  "),
            kv_span("skipped", snapshot.skipped_unreadable.to_string()),
            Span::raw("  "),
            kv_span("partial", snapshot.partial.to_string()),
        ]));
        lines.push(Line::from(vec![
            kv_span("disk_path", snapshot.disk.path.display().to_string()),
            Span::raw("  "),
            kv_span(
                "disk_available",
                snapshot
                    .disk
                    .available_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            Span::raw("  "),
            kv_span(
                "disk_used",
                snapshot
                    .disk
                    .used_percent
                    .map(|value| format!("{value:.0}%"))
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            Span::raw("  "),
            kv_span(
                "disk_state",
                match snapshot.disk.state {
                    DiskState::Healthy => "healthy",
                    DiskState::Low => "low",
                    DiskState::Unknown => "unknown",
                },
            ),
        ]));
    } else {
        lines.push(Line::from(
            "press r or Enter on Resources to scan; dashboard uses the last completed snapshot only",
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Resources").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_resource_storage(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let Some(snapshot) = &app.resources.snapshot else {
        render_resource_empty(frame, area, &app.resources.load_state);
        return;
    };
    let compact = area.width < 120;
    let rows = snapshot.visible_storage_rows(&app.resources.filter);
    let table_rows = rows.into_iter().map(|row| {
        let largest = row
            .largest_file
            .as_ref()
            .map(|file| format!("{} {}", format_bytes(file.bytes), file.path.display()))
            .unwrap_or_else(|| "-".to_string());
        if compact {
            Row::new(vec![
                Cell::from(row.category.clone()),
                Cell::from(format_bytes(row.total_bytes)),
                Cell::from(row.file_count.to_string()),
                Cell::from(if row.partial { "partial" } else { "ok" }),
            ])
        } else {
            Row::new(vec![
                Cell::from(row.category.clone()),
                Cell::from(row.path.display().to_string()),
                Cell::from(format_bytes(row.total_bytes)),
                Cell::from(row.file_count.to_string()),
                Cell::from(largest),
                Cell::from(format!(
                    "{}{}",
                    if row.exists { "exists" } else { "missing" },
                    if row.partial { "; partial" } else { "" }
                )),
            ])
        }
    });
    let (headers, widths) = if compact {
        (
            vec!["category", "size", "files", "status"],
            vec![
                Constraint::Length(14),
                Constraint::Length(12),
                Constraint::Length(8),
                Constraint::Min(12),
            ],
        )
    } else {
        (
            vec!["category", "path", "size", "files", "largest", "status"],
            vec![
                Constraint::Length(14),
                Constraint::Min(28),
                Constraint::Length(12),
                Constraint::Length(8),
                Constraint::Min(28),
                Constraint::Length(16),
            ],
        )
    };
    frame.render_widget(
        Table::new(table_rows, widths)
            .header(Row::new(headers).style(Style::default().add_modifier(Modifier::BOLD)))
            .block(Block::default().title("Storage").borders(Borders::ALL)),
        area,
    );
}

fn render_resource_processes(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let Some(snapshot) = &app.resources.snapshot else {
        render_resource_empty(frame, area, &app.resources.load_state);
        return;
    };
    let compact = area.width < 120;
    let rows = snapshot.visible_process_rows(&app.resources.filter);
    let table_rows = rows.into_iter().map(|row| {
        let pid = row
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string());
        let rss = row
            .rss_kib
            .map(|kib| format_bytes(kib.saturating_mul(1024)))
            .unwrap_or_else(|| "-".to_string());
        let cpu = row
            .cpu_percent
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "-".to_string());
        if compact {
            Row::new(vec![
                Cell::from(row.source.clone()),
                Cell::from(row.ref_label.clone()),
                Cell::from(pid),
                Cell::from(row.identity.label()),
            ])
        } else {
            Row::new(vec![
                Cell::from(row.source.clone()),
                Cell::from(row.ref_label.clone()),
                Cell::from(pid),
                Cell::from(row.state.clone()),
                Cell::from(rss),
                Cell::from(cpu),
                Cell::from(row.identity.label()),
                Cell::from(row.port_or_source.clone()),
                Cell::from(row.detail.clone()),
            ])
        }
    });
    let (headers, widths) = if compact {
        (
            vec!["source", "ref", "pid", "identity"],
            vec![
                Constraint::Length(10),
                Constraint::Length(16),
                Constraint::Length(10),
                Constraint::Min(22),
            ],
        )
    } else {
        (
            vec![
                "source",
                "ref",
                "pid",
                "state",
                "rss",
                "cpu",
                "identity",
                "port/source",
                "detail",
            ],
            vec![
                Constraint::Length(10),
                Constraint::Length(16),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(8),
                Constraint::Length(28),
                Constraint::Length(14),
                Constraint::Min(22),
            ],
        )
    };
    frame.render_widget(
        Table::new(table_rows, widths)
            .header(Row::new(headers).style(Style::default().add_modifier(Modifier::BOLD)))
            .block(Block::default().title("Processes").borders(Borders::ALL)),
        area,
    );
}

fn render_resource_warnings(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let Some(snapshot) = &app.resources.snapshot else {
        render_resource_empty(frame, area, &app.resources.load_state);
        return;
    };
    let compact = area.width < 110;
    let rows = snapshot.visible_warnings(&app.resources.filter);
    let table_rows = rows.into_iter().map(|warning| {
        let style = match warning.level.label() {
            "warn" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::DarkGray),
        };
        if compact {
            Row::new(vec![
                Cell::from(warning.level.label()),
                Cell::from(warning.source.clone()),
                Cell::from(warning.message.clone()),
            ])
            .style(style)
        } else {
            Row::new(vec![
                Cell::from(warning.level.label()),
                Cell::from(warning.source.clone()),
                Cell::from(warning.message.clone()),
                Cell::from(warning.detail.clone()),
            ])
            .style(style)
        }
    });
    let (headers, widths) = if compact {
        (
            vec!["level", "source", "message"],
            vec![
                Constraint::Length(8),
                Constraint::Length(14),
                Constraint::Min(24),
            ],
        )
    } else {
        (
            vec!["level", "source", "message", "detail"],
            vec![
                Constraint::Length(8),
                Constraint::Length(14),
                Constraint::Length(34),
                Constraint::Min(32),
            ],
        )
    };
    frame.render_widget(
        Table::new(table_rows, widths)
            .header(Row::new(headers).style(Style::default().add_modifier(Modifier::BOLD)))
            .block(Block::default().title("Warnings").borders(Borders::ALL)),
        area,
    );
}

fn render_resource_empty(frame: &mut Frame<'_>, area: Rect, state: &ResourceLoadState) {
    frame.render_widget(
        Paragraph::new(vec![
            line_kv("state", state.label()),
            Line::from("Resources are read-only. Press r to scan local runtime-home usage."),
        ])
        .block(Block::default().title("Resources").borders(Borders::ALL))
        .wrap(Wrap { trim: true }),
        area,
    );
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
