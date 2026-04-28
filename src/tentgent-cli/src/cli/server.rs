use std::{path::Path, process::Stdio};

use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use tentgent_core::{
    auth::{AuthManager, KeySource, KeyValidationState, Provider},
    server::{
        CloudProvider, LaunchMode, ServerInspection, ServerManager, ServerPrepareOutcome,
        ServerRunRequest, ServerSpec, ServerStopOutcome, ServerSummary,
    },
};
use tokio::process::Command;

use super::app::Cli;
use super::commands::{ServerCommands, ServerRunCommand};
use super::python_runtime::{require_python_interpreter, resolve_python_runtime};
use tentgent_core::runtime_assets::PythonRuntime;

#[derive(Debug, Clone)]
struct CloudRuntimeAuth {
    provider: Provider,
    secret: String,
}

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
            let cloud_auth = if inspection.spec.is_cloud() {
                Some(preflight_cloud_runtime_auth(&inspection.spec).await?)
            } else {
                ensure_local_runtime_launchable(&inspection.spec)?;
                None
            };
            let python_runtime = resolve_python_runtime()?;
            let python_interpreter =
                require_python_interpreter(&python_runtime, "python server interpreter")?;
            let inspection = launch_background_server_runtime(
                &manager,
                &python_runtime,
                &python_interpreter,
                &inspection,
                cloud_auth.as_ref(),
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
    if is_help_token(&command.runtime_ref) {
        print_server_subcommand_help("run")?;
        return Ok(());
    }

    let manager = ServerManager::new(command.home.as_deref()).into_diagnostic()?;
    let outcome = manager
        .prepare_run(ServerRunRequest {
            runtime_ref: command.runtime_ref,
            host: command.host,
            port: command.port,
            lazy_load: command.lazy_load,
            idle_seconds: command.idle_seconds,
        })
        .into_diagnostic()?;

    let detached = command.detach;
    render_server_spec_outcome(&outcome, detached);

    let cloud_auth = if outcome.spec.is_cloud() {
        Some(preflight_cloud_runtime_auth(&outcome.spec).await?)
    } else {
        ensure_local_runtime_launchable(&outcome.spec)?;
        None
    };

    let python_runtime = resolve_python_runtime()?;
    let python_interpreter =
        require_python_interpreter(&python_runtime, "python server interpreter")?;
    if detached {
        let inspection = inspection_from_prepare_outcome(&outcome);
        let inspection = launch_background_server_runtime(
            &manager,
            &python_runtime,
            &python_interpreter,
            &inspection,
            cloud_auth.as_ref(),
        )
        .await?;
        render_server_inspection("Server started", &inspection);
    } else {
        launch_foreground_server_runtime(
            &manager,
            &python_runtime,
            &python_interpreter,
            &outcome,
            cloud_auth.as_ref(),
        )
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
    if outcome.spec.is_cloud() {
        println!(
            "{} cloud provider auth will be verified before runtime launch.",
            style("checking").yellow().bold()
        );
    } else {
        println!(
            "{} launching the Python server in {} mode.",
            style("starting").green().bold(),
            if detached { "background" } else { "foreground" }
        );
    }

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
        println!(
            "{} No matching servers were found.\n",
            style("empty").yellow().bold()
        );
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
            "runtime",
            "provider",
            "model",
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
            Cell::new(server.spec.runtime_kind.as_str()),
            Cell::new(server.spec.provider_label()),
            Cell::new(server.spec.runtime_model_label()),
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

    table.add_row(vec![
        Cell::new("server_ref"),
        Cell::new(&inspection.spec.server_ref),
    ]);
    table.add_row(vec![
        Cell::new("short_ref"),
        Cell::new(&inspection.spec.short_ref),
    ]);
    table.add_row(vec![
        Cell::new("runtime"),
        Cell::new(inspection.spec.runtime_kind.as_str()),
    ]);
    if inspection.spec.is_cloud() {
        table.add_row(vec![
            Cell::new("provider"),
            Cell::new(inspection.spec.provider_label()),
        ]);
        table.add_row(vec![
            Cell::new("provider_model"),
            Cell::new(inspection.spec.runtime_model_label()),
        ]);
    } else {
        table.add_row(vec![
            Cell::new("model_ref"),
            Cell::new(inspection.spec.runtime_model_label()),
        ]);
    }
    table.add_row(vec![
        Cell::new("status"),
        Cell::new(if inspection.running {
            "running"
        } else {
            "stopped"
        }),
    ]);
    table.add_row(vec![
        Cell::new("home"),
        Cell::new(inspection.home_dir.display().to_string()),
    ]);
    table.add_row(vec![Cell::new("host"), Cell::new(&inspection.spec.host)]);
    table.add_row(vec![Cell::new("port"), Cell::new(inspection.spec.port)]);
    table.add_row(vec![
        Cell::new("lazy_load"),
        Cell::new(if inspection.spec.lazy_load {
            "true"
        } else {
            "false"
        }),
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
    table.add_row(vec![
        Cell::new("created_at"),
        Cell::new(&inspection.spec.created_at),
    ]);
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
    python_runtime: &PythonRuntime,
    python_interpreter: &Path,
    outcome: &ServerPrepareOutcome,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> miette::Result<()> {
    let mut process = server_process_command(
        python_runtime,
        python_interpreter,
        &outcome.spec,
        &outcome.home_dir,
        cloud_auth,
    )?;
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
    python_runtime: &PythonRuntime,
    python_interpreter: &Path,
    inspection: &ServerInspection,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> miette::Result<ServerInspection> {
    let mut process = Command::new("sh");
    process
        .current_dir(python_runtime.project_dir())
        .env("TENTGENT_STDOUT_LOG", &inspection.stdout_log_path)
        .env("TENTGENT_STDERR_LOG", &inspection.stderr_log_path)
        .arg("-c")
        .arg(
            "nohup \"$@\" >>\"$TENTGENT_STDOUT_LOG\" 2>>\"$TENTGENT_STDERR_LOG\" < /dev/null & echo $!",
        )
        .arg("sh")
        .arg(python_interpreter)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);

    append_server_runtime_args(
        &mut process,
        &inspection.spec,
        &inspection.home_dir,
        cloud_auth,
    )?;

    let output = process.output().await.into_diagnostic()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("status {}", output.status)
        } else {
            stderr
        };
        return Err(miette!(
            "failed to launch background server runtime: {detail}"
        ));
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
    python_runtime: &PythonRuntime,
    python_interpreter: &Path,
    spec: &ServerSpec,
    home_dir: &Path,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> miette::Result<Command> {
    let mut process = Command::new(python_interpreter);
    process.current_dir(python_runtime.project_dir());
    append_server_runtime_args(&mut process, spec, home_dir, cloud_auth)?;
    Ok(process)
}

fn append_server_runtime_args(
    process: &mut Command,
    spec: &ServerSpec,
    home_dir: &Path,
    cloud_auth: Option<&CloudRuntimeAuth>,
) -> miette::Result<()> {
    process
        .arg("-m")
        .arg("tentgent_daemon.cli.server")
        .arg("--server-ref")
        .arg(&spec.server_ref)
        .arg("--runtime-kind")
        .arg(spec.runtime_kind.as_str())
        .arg("--host")
        .arg(&spec.host)
        .arg("--port")
        .arg(spec.port.to_string())
        .arg("--home")
        .arg(home_dir);

    if spec.is_cloud() {
        let provider = spec.provider.ok_or_else(|| {
            miette!(
                "cloud server spec `{}` is missing provider metadata",
                spec.short_ref
            )
        })?;
        let provider_model = spec.provider_model.as_deref().ok_or_else(|| {
            miette!(
                "cloud server spec `{}` is missing provider_model metadata",
                spec.short_ref
            )
        })?;
        let cloud_auth = cloud_auth.ok_or_else(|| {
            miette!(
                "cloud server spec `{}` is missing launch-time provider auth",
                spec.short_ref
            )
        })?;
        process
            .env(cloud_auth.provider.env_var(), &cloud_auth.secret)
            .arg("--provider")
            .arg(provider.as_str())
            .arg("--provider-model")
            .arg(provider_model);
    } else {
        process
            .arg("--model-ref")
            .arg(ensure_local_runtime_launchable(spec)?);
    }

    if spec.lazy_load {
        process.arg("--lazy-load");
    }

    if let Some(idle_seconds) = spec.idle_seconds {
        process.arg("--idle-seconds").arg(idle_seconds.to_string());
    }

    Ok(())
}

fn ensure_local_runtime_launchable(spec: &ServerSpec) -> miette::Result<&str> {
    spec.local_model_ref().ok_or_else(|| {
        if spec.is_cloud() {
            miette!(
                "cloud server spec `{}` cannot be launched through the local model path",
                spec.short_ref
            )
        } else {
            miette!(
                "local server spec `{}` is missing model_ref",
                spec.short_ref
            )
        }
    })
}

async fn preflight_cloud_runtime_auth(spec: &ServerSpec) -> miette::Result<CloudRuntimeAuth> {
    let cloud_provider = spec.provider.ok_or_else(|| {
        miette!(
            "cloud server spec `{}` is missing provider metadata",
            spec.short_ref
        )
    })?;
    let provider = auth_provider_for_cloud(cloud_provider);
    let auth = AuthManager::new().into_diagnostic()?;
    let Some((source, secret)) = auth.effective_secret(provider).into_diagnostic()? else {
        return Err(miette!(
            "{} key is missing for cloud server `{}`; run `tentgent auth {} set` or set `{}` before launch",
            provider.display_name(),
            spec.short_ref,
            provider.cli_name(),
            provider.env_var()
        ));
    };

    match auth.validate_secret(provider, &secret).await {
        KeyValidationState::Verified => {
            render_cloud_auth_preflight(provider, source);
            Ok(CloudRuntimeAuth { provider, secret })
        }
        KeyValidationState::Invalid { reason } => Err(miette!(
            "{} key from {} is invalid for cloud server `{}`: {}",
            provider.display_name(),
            source,
            spec.short_ref,
            reason
        )),
        KeyValidationState::Unknown { reason } => Err(miette!(
            "{} key from {} could not be verified for cloud server `{}`: {}",
            provider.display_name(),
            source,
            spec.short_ref,
            reason
        )),
        KeyValidationState::Missing => Err(miette!(
            "{} key is missing for cloud server `{}`; run `tentgent auth {} set` or set `{}` before launch",
            provider.display_name(),
            spec.short_ref,
            provider.cli_name(),
            provider.env_var()
        )),
    }
}

fn render_cloud_auth_preflight(provider: Provider, source: KeySource) {
    println!(
        "{} {} key verified from {} for cloud runtime.",
        style("verified").green().bold(),
        provider.display_name(),
        source
    );
}

fn auth_provider_for_cloud(provider: CloudProvider) -> Provider {
    match provider {
        CloudProvider::OpenAI => Provider::OpenAI,
        CloudProvider::Anthropic => Provider::Anthropic,
    }
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
