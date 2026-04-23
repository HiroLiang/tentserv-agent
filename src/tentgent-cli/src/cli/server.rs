use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use tentgent_core::server::{
    LaunchMode, ServerInspection, ServerManager, ServerPrepareOutcome, ServerRunRequest,
    ServerStopOutcome, ServerSummary,
};
use tokio::process::Command;

use super::app::Cli;
use super::commands::{ServerCommands, ServerRunCommand};

pub async fn handle_server_command(action: ServerCommands) -> miette::Result<()> {
    match action {
        ServerCommands::Run(command) => run_server(command).await?,
        ServerCommands::Ls { home } => {
            let manager = ServerManager::new(home.as_deref()).into_diagnostic()?;
            let servers = manager.list().into_diagnostic()?;
            render_server_list("Registered servers", &servers);
        }
        ServerCommands::Ps { home } => {
            let manager = ServerManager::new(home.as_deref()).into_diagnostic()?;
            let servers = manager.list_running().into_diagnostic()?;
            render_server_list("Running servers", &servers);
        }
        ServerCommands::Inspect { reference, home } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("inspect")?;
                return Ok(());
            }

            let manager = ServerManager::new(home.as_deref()).into_diagnostic()?;
            let inspection = manager.inspect(&reference).into_diagnostic()?;
            render_server_inspection("Server inspection", &inspection);
        }
        ServerCommands::Start {
            reference,
            home,
            details,
        } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("start")?;
                return Ok(());
            }

            let manager = ServerManager::new(home.as_deref()).into_diagnostic()?;
            let inspection = manager.resolve_for_start(&reference).into_diagnostic()?;
            let python_project = resolve_python_project_dir();
            let python_interpreter = resolve_python_interpreter(&python_project)?;
            let inspection = launch_background_server_runtime(
                &manager,
                &python_project,
                &python_interpreter,
                &inspection,
            )
            .await?;
            render_server_started(&inspection, details);
        }
        ServerCommands::Stop {
            reference,
            home,
            details,
        } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("stop")?;
                return Ok(());
            }

            let manager = ServerManager::new(home.as_deref()).into_diagnostic()?;
            let outcome = manager.stop(&reference).into_diagnostic()?;
            render_server_stop(&outcome, details);
        }
        ServerCommands::Rm {
            reference,
            home,
            details,
        } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("rm")?;
                return Ok(());
            }

            let manager = ServerManager::new(home.as_deref()).into_diagnostic()?;
            let outcome = manager.remove(&reference).into_diagnostic()?;
            render_server_removed(&outcome.inspection, details);
        }
    }

    Ok(())
}

async fn run_server(command: ServerRunCommand) -> miette::Result<()> {
    if is_help_token(&command.model_ref) {
        print_server_subcommand_help("run")?;
        return Ok(());
    }

    let manager = ServerManager::new(command.home.as_deref()).into_diagnostic()?;
    let outcome = manager
        .prepare_run(ServerRunRequest {
            model_ref: command.model_ref,
            host: command.host,
            port: command.port,
            lazy_load: command.lazy_load,
            idle_seconds: command.idle_seconds,
        })
        .into_diagnostic()?;

    let detached = command.detach;
    render_server_spec_outcome(&outcome, detached);

    let python_project = resolve_python_project_dir();
    let python_interpreter = resolve_python_interpreter(&python_project)?;
    if detached {
        let inspection = inspection_from_prepare_outcome(&outcome);
        let inspection = launch_background_server_runtime(
            &manager,
            &python_project,
            &python_interpreter,
            &inspection,
        )
        .await?;
        render_server_inspection("Server started", &inspection);
    } else {
        launch_foreground_server_runtime(&manager, &python_project, &python_interpreter, &outcome)
            .await?;
    }

    Ok(())
}

fn render_server_spec_outcome(outcome: &ServerPrepareOutcome, detached: bool) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style(if outcome.created {
            "Server spec created"
        } else {
            "Server spec reused"
        })
        .bold()
    );
    println!(
        "{} server {} at {}",
        if outcome.created {
            style("stored").green().bold()
        } else {
            style("reused").yellow().bold()
        },
        outcome.spec.short_ref,
        outcome.spec_path.display()
    );
    println!(
        "{} launching the Python server in {} mode.",
        style("starting").green().bold(),
        if detached { "background" } else { "foreground" }
    );

    let inspection = inspection_from_prepare_outcome(outcome);
    println!("{}", render_server_table(&inspection));
    println!();
}

fn inspection_from_prepare_outcome(outcome: &ServerPrepareOutcome) -> ServerInspection {
    ServerInspection {
        spec: outcome.spec.clone(),
        home_dir: outcome.home_dir.clone(),
        server_dir: outcome.server_dir.clone(),
        spec_path: outcome.spec_path.clone(),
        process_path: outcome.process_path.clone(),
        stdout_log_path: outcome.stdout_log_path.clone(),
        stderr_log_path: outcome.stderr_log_path.clone(),
        running: false,
        process: None,
    }
}

fn render_server_list(title: &str, servers: &[ServerSummary]) {
    println!("{} {}", style("==>").cyan().bold(), style(title).bold());

    if servers.is_empty() {
        println!("{} No matching servers were found.\n", style("empty").yellow().bold());
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "short_ref",
            "status",
            "mode",
            "model_ref",
            "host",
            "port",
            "pid",
        ]);

    for server in servers {
        let mode = if server.running {
            server
                .process
                .as_ref()
                .map(|process| process.launch_mode.as_str())
                .unwrap_or("-")
        } else {
            "-"
        };
        let pid = if server.running {
            server
                .process
                .as_ref()
                .map(|process| process.pid.to_string())
                .unwrap_or_else(|| "-".to_string())
        } else {
            "-".to_string()
        };

        table.add_row(vec![
            Cell::new(&server.spec.short_ref),
            Cell::new(if server.running { "running" } else { "stopped" }),
            Cell::new(mode),
            Cell::new(&server.spec.model_ref),
            Cell::new(&server.spec.host),
            Cell::new(server.spec.port),
            Cell::new(pid),
        ]);
    }

    println!("{table}");
    println!();
}

fn render_server_inspection(title: &str, inspection: &ServerInspection) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style(title).bold(),
        style(&inspection.spec.short_ref).bold()
    );
    println!("{}", render_server_table(inspection));
    println!();
}

fn render_server_started(inspection: &ServerInspection, details: bool) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Server started").bold(),
        inspection.spec.short_ref
    );
    let pid = inspection
        .process
        .as_ref()
        .map(|process| process.pid.to_string())
        .unwrap_or_else(|| "(unknown)".to_string());
    println!(
        "{} server {} pid {}",
        style("started").green().bold(),
        inspection.spec.short_ref,
        pid
    );
    if details {
        println!("{}", render_server_table(inspection));
        println!();
    }
}

fn render_server_stop(outcome: &ServerStopOutcome, details: bool) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Server stopped").bold()
    );
    println!(
        "{} server {} pid {}",
        style("stopped").red().bold(),
        outcome.inspection.spec.short_ref,
        outcome.stopped_pid
    );
    if details {
        println!("{}", render_server_table(&outcome.inspection));
        println!();
    }
}

fn render_server_removed(inspection: &ServerInspection, details: bool) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Server removed").bold()
    );
    println!(
        "{} server {} from {}",
        style("removed").red().bold(),
        inspection.spec.short_ref,
        inspection.server_dir.display()
    );
    if details {
        println!("{}", render_server_table(inspection));
        println!();
    }
}

fn render_server_table(inspection: &ServerInspection) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);

    table.add_row(vec![Cell::new("server_ref"), Cell::new(&inspection.spec.server_ref)]);
    table.add_row(vec![Cell::new("short_ref"), Cell::new(&inspection.spec.short_ref)]);
    table.add_row(vec![Cell::new("model_ref"), Cell::new(&inspection.spec.model_ref)]);
    table.add_row(vec![
        Cell::new("status"),
        Cell::new(if inspection.running { "running" } else { "stopped" }),
    ]);
    table.add_row(vec![Cell::new("home"), Cell::new(inspection.home_dir.display().to_string())]);
    table.add_row(vec![Cell::new("host"), Cell::new(&inspection.spec.host)]);
    table.add_row(vec![Cell::new("port"), Cell::new(inspection.spec.port)]);
    table.add_row(vec![
        Cell::new("lazy_load"),
        Cell::new(if inspection.spec.lazy_load { "true" } else { "false" }),
    ]);
    table.add_row(vec![
        Cell::new("idle_seconds"),
        Cell::new(
            inspection
                .spec
                .idle_seconds
                .map(|seconds| seconds.to_string())
                .unwrap_or_else(|| "(not set)".to_string()),
        ),
    ]);
    table.add_row(vec![Cell::new("created_at"), Cell::new(&inspection.spec.created_at)]);
    table.add_row(vec![
        Cell::new("launch_mode"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.launch_mode.as_str().to_string())
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
        Cell::new("server_dir"),
        Cell::new(inspection.server_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("spec_path"),
        Cell::new(inspection.spec_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("process_path"),
        Cell::new(inspection.process_path.display().to_string()),
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

async fn launch_foreground_server_runtime(
    manager: &ServerManager,
    python_project: &Path,
    python_interpreter: &Path,
    outcome: &ServerPrepareOutcome,
) -> miette::Result<()> {
    let mut process = server_process_command(python_project, python_interpreter, &outcome.spec, &outcome.home_dir);
    process
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    let mut child = process.spawn().into_diagnostic()?;
    let pid = child
        .id()
        .ok_or_else(|| miette!("failed to obtain server process pid"))?;
    manager
        .record_process_start(&outcome.spec.server_ref, pid, LaunchMode::Foreground)
        .into_diagnostic()?;

    let status = child.wait().await.into_diagnostic()?;
    manager
        .clear_process_if_matches(&outcome.spec.server_ref, Some(pid))
        .into_diagnostic()?;
    if !status.success() {
        return Err(miette!("server runtime exited with status {status}"));
    }

    Ok(())
}

async fn launch_background_server_runtime(
    manager: &ServerManager,
    python_project: &Path,
    python_interpreter: &Path,
    inspection: &ServerInspection,
) -> miette::Result<ServerInspection> {
    let mut process = Command::new("sh");
    process
        .current_dir(python_project)
        .env("TENTGENT_STDOUT_LOG", &inspection.stdout_log_path)
        .env("TENTGENT_STDERR_LOG", &inspection.stderr_log_path)
        .arg("-c")
        .arg(
            "nohup \"$@\" >>\"$TENTGENT_STDOUT_LOG\" 2>>\"$TENTGENT_STDERR_LOG\" < /dev/null & echo $!",
        )
        .arg("sh")
        .arg(python_interpreter)
        .arg("-m")
        .arg("tentgent_daemon.cli.server")
        .arg("--server-ref")
        .arg(&inspection.spec.server_ref)
        .arg("--model-ref")
        .arg(&inspection.spec.model_ref)
        .arg("--host")
        .arg(&inspection.spec.host)
        .arg("--port")
        .arg(inspection.spec.port.to_string())
        .arg("--home")
        .arg(&inspection.home_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);

    if inspection.spec.lazy_load {
        process.arg("--lazy-load");
    }

    if let Some(idle_seconds) = inspection.spec.idle_seconds {
        process.arg("--idle-seconds").arg(idle_seconds.to_string());
    }

    let output = process.output().await.into_diagnostic()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("status {}", output.status)
        } else {
            stderr
        };
        return Err(miette!("failed to launch background server runtime: {detail}"));
    }

    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .into_diagnostic()?;

    let inspection = manager
        .record_process_start(&inspection.spec.server_ref, pid, LaunchMode::Background)
        .into_diagnostic()?;
    Ok(inspection)
}

fn server_process_command(
    python_project: &Path,
    python_interpreter: &Path,
    spec: &tentgent_core::server::ServerSpec,
    home_dir: &Path,
) -> Command {
    let mut process = Command::new(python_interpreter);
    process
        .current_dir(python_project)
        .arg("-m")
        .arg("tentgent_daemon.cli.server")
        .arg("--server-ref")
        .arg(&spec.server_ref)
        .arg("--model-ref")
        .arg(&spec.model_ref)
        .arg("--host")
        .arg(&spec.host)
        .arg("--port")
        .arg(spec.port.to_string())
        .arg("--home")
        .arg(home_dir);

    if spec.lazy_load {
        process.arg("--lazy-load");
    }

    if let Some(idle_seconds) = spec.idle_seconds {
        process.arg("--idle-seconds").arg(idle_seconds.to_string());
    }

    process
}

fn resolve_python_project_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("python/tentgent-daemon")
}

fn resolve_python_interpreter(project_dir: &Path) -> miette::Result<PathBuf> {
    let interpreter = project_dir.join(".venv/bin/python");
    if interpreter.exists() {
        return Ok(interpreter);
    }

    Err(miette!(
        "python server interpreter is missing at `{}`; initialize the Python subproject environment first",
        interpreter.display()
    ))
}

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}

fn print_server_subcommand_help(name: &str) -> miette::Result<()> {
    let mut root = Cli::command();
    let server = root
        .find_subcommand_mut("server")
        .ok_or_else(|| miette!("server command metadata is unavailable"))?;
    let subcommand = server
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("server subcommand `{name}` is unavailable"))?;
    subcommand.print_long_help().into_diagnostic()?;
    println!();
    Ok(())
}
