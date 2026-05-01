use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::IntoDiagnostic;
use tentgent_core::daemon::{DaemonInspection, DaemonManager, DaemonRunRequest, DaemonStopOutcome};
use tentgent_http::{
    security::{check_bind_safety, DaemonSecurityConfig},
    DaemonHttpServer, DaemonHttpState,
};

use super::commands::{DaemonCommands, DaemonRunCommand};

pub async fn handle_daemon_command(action: DaemonCommands) -> miette::Result<()> {
    match action {
        DaemonCommands::Run(command) => run_daemon(command).await?,
        DaemonCommands::Status { home } => {
            let manager = DaemonManager::new(home.as_deref()).into_diagnostic()?;
            let inspection = manager.status().into_diagnostic()?;
            render_daemon_inspection("Daemon status", &inspection);
        }
        DaemonCommands::Stop { home } => {
            let manager = DaemonManager::new(home.as_deref()).into_diagnostic()?;
            let outcome = manager.stop().into_diagnostic()?;
            render_daemon_stop(&outcome);
        }
    }

    Ok(())
}

async fn run_daemon(command: DaemonRunCommand) -> miette::Result<()> {
    let manager = DaemonManager::new(command.home.as_deref()).into_diagnostic()?;
    let spec = manager
        .prepare_run(DaemonRunRequest {
            host: command.host,
            port: command.port,
        })
        .into_diagnostic()?;
    let security = DaemonSecurityConfig::from_env();
    let bind_safety = check_bind_safety(
        &spec.host,
        security.token_enabled(),
        command.allow_unsafe_bind,
    )?;
    for warning in bind_safety.warnings {
        println!("{} {}", style("warning").yellow().bold(), warning);
    }
    let server = DaemonHttpServer::bind(spec.host, spec.port).await?;
    let pid = std::process::id();
    let inspection = manager
        .record_process_start(pid, server.host().to_string(), server.port())
        .into_diagnostic()?;

    render_daemon_inspection("Daemon started", &inspection);
    println!(
        "{} listening on {}; try GET /healthz or GET /v1/status.",
        style("http").green().bold(),
        server.bind_label()
    );
    println!("{} press Ctrl-C to stop.", style("note").yellow().bold());

    let serve_result = tokio::select! {
        result = server.serve(DaemonHttpState::with_security(inspection, security)) => Some(result),
        signal = tokio::signal::ctrl_c() => {
            signal.into_diagnostic()?;
            None
        }
    };
    manager
        .clear_process_if_matches(Some(pid))
        .into_diagnostic()?;
    if let Some(result) = serve_result {
        result?;
    }
    println!("{} daemon stopped.", style("==>").cyan().bold());

    Ok(())
}

fn render_daemon_stop(outcome: &DaemonStopOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Daemon stopped").bold()
    );
    println!(
        "{} pid {}",
        style("stopped").green().bold(),
        outcome.stopped_pid
    );
    println!("{}", render_daemon_table(&outcome.inspection));
}

fn render_daemon_inspection(title: &str, inspection: &DaemonInspection) {
    println!("{} {}", style("==>").cyan().bold(), style(title).bold());
    println!("{}", render_daemon_table(inspection));
}

fn render_daemon_table(inspection: &DaemonInspection) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);

    table.add_row(vec![
        Cell::new("status"),
        Cell::new(if inspection.running {
            "running"
        } else {
            "stopped"
        }),
    ]);
    table.add_row(vec![
        Cell::new("host"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.host.clone())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("port"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.port.to_string())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("pid"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.pid.to_string())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("started_at"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.started_at.clone())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("home"),
        Cell::new(inspection.home_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("runtime_dir"),
        Cell::new(inspection.runtime_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("log_dir"),
        Cell::new(inspection.log_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("process_path"),
        Cell::new(inspection.process_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("pid_path"),
        Cell::new(inspection.pid_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("stdout_log"),
        Cell::new(inspection.stdout_log_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("stderr_log"),
        Cell::new(inspection.stderr_log_path.display().to_string()),
    ]);

    table
}
