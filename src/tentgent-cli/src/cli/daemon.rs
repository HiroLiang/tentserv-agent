use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::IntoDiagnostic;
use tentgent_core::daemon as core_daemon;
use tentgent_http::{
    security::{DaemonSecurityConfig, DAEMON_TOKEN_ENV_VAR},
    DaemonHttpServer, DaemonHttpState,
};
use tentgent_kernel::{
    features::daemon::{
        domain::{
            daemon_url, DaemonBind, DaemonInspection as KernelDaemonInspection,
            DaemonProcessMetadata as KernelDaemonProcessMetadata,
            DaemonWarning as KernelDaemonWarning, DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT,
        },
        infra::{
            FileDaemonStateStore, ReqwestDaemonHttpReadinessProbe, StdDaemonBindSafetyChecker,
            StdDaemonDetachedLauncher, StdDaemonProcessController, StdDaemonProcessProbe,
            StdDaemonStoreLayoutInitializer, SystemDaemonClock,
        },
        usecases::{
            DaemonClearProcessRequest, DaemonDetachedStartRequest, DaemonDetachedStartUseCase,
            DaemonInspectionMode, DaemonLifecycleUseCase, DaemonPrepareRunRequest,
            DaemonReadinessToken, DaemonRecordProcessStartRequest, DaemonStatusRequest,
            DaemonStatusUseCase, DaemonStopRequest, StdDaemonUseCase,
        },
    },
    foundation::layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver},
};

use super::commands::{DaemonCommands, DaemonRunCommand, DaemonStartCommand};

const DAEMON_URL_ENV_VAR: &str = "TENTGENT_DAEMON_URL";
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const DAEMON_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

type DaemonInspection = core_daemon::DaemonInspection;
type DaemonWarning = core_daemon::DaemonWarning;

struct CliDaemonKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdDaemonStoreLayoutInitializer,
    state_store: FileDaemonStateStore,
    process_probe: StdDaemonProcessProbe,
    process_controller: StdDaemonProcessController,
    bind_safety_checker: StdDaemonBindSafetyChecker,
    detached_launcher: StdDaemonDetachedLauncher,
    readiness_probe: ReqwestDaemonHttpReadinessProbe,
    clock: SystemDaemonClock,
}

impl CliDaemonKernel {
    fn new() -> miette::Result<Self> {
        Ok(Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdDaemonStoreLayoutInitializer,
            state_store: FileDaemonStateStore,
            process_probe: StdDaemonProcessProbe,
            process_controller: StdDaemonProcessController::default(),
            bind_safety_checker: StdDaemonBindSafetyChecker,
            detached_launcher: StdDaemonDetachedLauncher,
            readiness_probe: ReqwestDaemonHttpReadinessProbe::new(DAEMON_PROBE_TIMEOUT)
                .into_diagnostic()?,
            clock: SystemDaemonClock,
        })
    }

    fn usecase(&self) -> StdDaemonUseCase<'_> {
        StdDaemonUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.state_store,
            &self.process_probe,
            &self.process_controller,
            &self.bind_safety_checker,
            &self.detached_launcher,
            &self.readiness_probe,
            &self.clock,
        )
    }
}

fn daemon_layout(home: Option<PathBuf>, mode: LayoutResolveMode) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: home,
        data_root_dir: None,
    }
}

pub(crate) fn daemon_status_with_cleanup(home: Option<&Path>) -> miette::Result<DaemonInspection> {
    let kernel = CliDaemonKernel::new()?;
    let daemon = kernel.usecase();
    let status = daemon
        .daemon_status(DaemonStatusRequest {
            layout: daemon_layout(home.map(Path::to_path_buf), LayoutResolveMode::ReadOnly),
            mode: DaemonInspectionMode::CleanupStale,
        })
        .into_diagnostic()?;
    Ok(core_daemon_inspection(status.inspection))
}

pub async fn handle_daemon_command(action: DaemonCommands) -> miette::Result<()> {
    match action {
        DaemonCommands::Run(command) if command.detach => {
            start_daemon(DetachedDaemonOptions::from_run(command)).await?
        }
        DaemonCommands::Run(command) => run_daemon(command).await?,
        DaemonCommands::Start(command) => {
            start_daemon(DetachedDaemonOptions::from_start(command)).await?
        }
        DaemonCommands::Status { home } => {
            let inspection = daemon_status_with_cleanup(home.as_deref())?;
            render_daemon_inspection("Daemon status", &inspection);
            render_daemon_guidance(&inspection);
        }
        DaemonCommands::Stop { home } => {
            let kernel = CliDaemonKernel::new()?;
            let daemon = kernel.usecase();
            let outcome = daemon
                .stop_daemon(DaemonStopRequest {
                    layout: daemon_layout(home, LayoutResolveMode::Create),
                })
                .into_diagnostic()?;
            let inspection = core_daemon_inspection(outcome.inspection);
            render_daemon_stop(outcome.stopped_pid, &inspection);
        }
    }

    Ok(())
}

async fn run_daemon(command: DaemonRunCommand) -> miette::Result<()> {
    let kernel = CliDaemonKernel::new()?;
    let daemon = kernel.usecase();
    let security = DaemonSecurityConfig::from_env();
    let prepared = daemon
        .prepare_run(DaemonPrepareRunRequest {
            layout: daemon_layout(command.home.clone(), LayoutResolveMode::Create),
            host: command.host,
            port: command.port,
            token_enabled: security.token_enabled(),
            allow_unsafe_bind: command.allow_unsafe_bind,
        })
        .into_diagnostic()?;
    for warning in &prepared.bind_warnings {
        println!("{} {}", style("warning").yellow().bold(), warning);
    }
    let server = DaemonHttpServer::bind(prepared.bind.host.clone(), prepared.bind.port).await?;
    let pid = std::process::id();
    let recorded = daemon
        .record_process_start(DaemonRecordProcessStartRequest {
            layout: daemon_layout(
                Some(prepared.layout.home_dir.clone()),
                LayoutResolveMode::ReadOnly,
            ),
            pid,
            bind: DaemonBind {
                host: server.host().to_string(),
                port: server.port(),
            },
        })
        .into_diagnostic()?;
    let inspection = core_daemon_inspection(recorded.inspection);

    render_daemon_inspection("Daemon started", &inspection);
    println!(
        "{} listening on {}; try GET /healthz or GET /v1/status.",
        style("http").green().bold(),
        server.bind_label()
    );
    println!("{} press Ctrl-C to stop.", style("note").yellow().bold());
    let home_dir = inspection.home_dir.clone();

    let serve_result = tokio::select! {
        result = server.serve(DaemonHttpState::with_security(inspection, security)) => Some(result),
        signal = tokio::signal::ctrl_c() => {
            signal.into_diagnostic()?;
            None
        }
    };
    daemon
        .clear_process_if_matches(DaemonClearProcessRequest {
            layout: daemon_layout(Some(home_dir), LayoutResolveMode::ReadOnly),
            expected_pid: Some(pid),
        })
        .into_diagnostic()?;
    if let Some(result) = serve_result {
        result?;
    }
    println!("{} daemon stopped.", style("==>").cyan().bold());

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetachedDaemonOptions {
    pub(crate) home: Option<PathBuf>,
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) allow_unsafe_bind: bool,
}

impl DetachedDaemonOptions {
    pub(crate) fn from_run(command: DaemonRunCommand) -> Self {
        Self {
            home: command.home,
            host: command.host,
            port: command.port,
            allow_unsafe_bind: command.allow_unsafe_bind,
        }
    }

    pub(crate) fn from_start(command: DaemonStartCommand) -> Self {
        Self {
            home: command.home,
            host: command.host,
            port: command.port,
            allow_unsafe_bind: command.allow_unsafe_bind,
        }
    }
}

#[derive(Debug)]
pub(crate) struct DetachedDaemonStartOutcome {
    pub(crate) inspection: DaemonInspection,
    pub(crate) daemon_url: String,
    pub(crate) launched_pid: Option<u32>,
    pub(crate) stdout_log_path: PathBuf,
    pub(crate) stderr_log_path: PathBuf,
    pub(crate) status_warning: Option<String>,
    pub(crate) bind_warnings: Vec<String>,
    pub(crate) already_running: bool,
}

async fn start_daemon(options: DetachedDaemonOptions) -> miette::Result<()> {
    let outcome = start_daemon_detached(options).await?;
    let title = if outcome.already_running {
        "Daemon already running"
    } else {
        "Daemon started"
    };
    for warning in &outcome.bind_warnings {
        println!("{} {}", style("warning").yellow().bold(), warning);
    }
    render_daemon_inspection(title, &outcome.inspection);
    println!(
        "{} listening on {}; try GET /healthz or GET /v1/status.",
        style("http").green().bold(),
        outcome.daemon_url
    );
    if let Some(pid) = outcome.launched_pid {
        println!(
            "{} launched background pid {}.",
            style("pid").cyan().bold(),
            pid
        );
    }
    println!(
        "{} stdout {}",
        style("log").cyan().bold(),
        outcome.stdout_log_path.display()
    );
    println!(
        "{} stderr {}",
        style("log").cyan().bold(),
        outcome.stderr_log_path.display()
    );
    if let Some(warning) = outcome.status_warning {
        println!("{} {}", style("warning").yellow().bold(), warning);
    }

    Ok(())
}

pub(crate) async fn start_daemon_detached(
    options: DetachedDaemonOptions,
) -> miette::Result<DetachedDaemonStartOutcome> {
    let security = DaemonSecurityConfig::from_env();
    let kernel = CliDaemonKernel::new()?;
    let daemon = kernel.usecase();
    let token = read_daemon_token_from_env().and_then(DaemonReadinessToken::parse);
    let result = daemon
        .start_daemon_detached(DaemonDetachedStartRequest {
            layout: daemon_layout(options.home, LayoutResolveMode::Create),
            host: options.host,
            port: options.port,
            token_enabled: security.token_enabled(),
            allow_unsafe_bind: options.allow_unsafe_bind,
            executable: env::current_exe().into_diagnostic()?,
            status_probe_token: token,
            startup_timeout: DAEMON_STARTUP_TIMEOUT,
        })
        .await
        .into_diagnostic()?;

    Ok(DetachedDaemonStartOutcome {
        daemon_url: result.daemon_url,
        inspection: core_daemon_inspection(result.inspection),
        launched_pid: result.launched_pid,
        stdout_log_path: result.stdout_log_path,
        stderr_log_path: result.stderr_log_path,
        status_warning: result.status_warning,
        bind_warnings: result.bind_warnings,
        already_running: result.already_running,
    })
}

fn core_daemon_inspection(inspection: KernelDaemonInspection) -> DaemonInspection {
    DaemonInspection {
        home_dir: inspection.home_dir,
        runtime_dir: inspection.runtime_dir,
        log_dir: inspection.log_dir,
        process_path: inspection.process_path,
        pid_path: inspection.pid_path,
        stdout_log_path: inspection.stdout_log_path,
        stderr_log_path: inspection.stderr_log_path,
        running: inspection.running,
        process: inspection.process.map(core_daemon_process_metadata),
        warnings: inspection
            .warnings
            .into_iter()
            .map(core_daemon_warning)
            .collect(),
    }
}

fn core_daemon_process_metadata(
    process: KernelDaemonProcessMetadata,
) -> core_daemon::DaemonProcessMetadata {
    core_daemon::DaemonProcessMetadata {
        pid: process.pid,
        host: process.host,
        port: process.port,
        started_at: process.started_at,
    }
}

fn core_daemon_warning(warning: KernelDaemonWarning) -> DaemonWarning {
    DaemonWarning {
        code: warning.code,
        message: warning.message,
        path: warning.path,
    }
}

fn read_daemon_token_from_env() -> Option<String> {
    env::var(DAEMON_TOKEN_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn daemon_url_from_process_metadata(inspection: &DaemonInspection) -> Option<String> {
    inspection
        .process
        .as_ref()
        .map(|process| daemon_url(&process.host, process.port))
}

fn daemon_url_from_inspection(inspection: &DaemonInspection) -> String {
    if let Some(value) = read_env_string(DAEMON_URL_ENV_VAR) {
        return value;
    }
    if let Some(value) = daemon_url_from_process_metadata(inspection) {
        return value;
    }
    daemon_url(DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT)
}

fn read_env_string(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn render_daemon_stop(stopped_pid: u32, inspection: &DaemonInspection) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Daemon stopped").bold()
    );
    println!("{} pid {}", style("stopped").green().bold(), stopped_pid);
    println!("{}", render_daemon_table(inspection));
}

fn render_daemon_inspection(title: &str, inspection: &DaemonInspection) {
    println!("{} {}", style("==>").cyan().bold(), style(title).bold());
    println!("{}", render_daemon_table(inspection));
    render_daemon_warnings(&inspection.warnings);
}

fn render_daemon_guidance(inspection: &DaemonInspection) {
    for line in daemon_guidance_lines(inspection) {
        println!("{line}");
    }
}

fn daemon_guidance_lines(inspection: &DaemonInspection) -> Vec<String> {
    let daemon_url = daemon_url_from_inspection(inspection);
    if inspection.running {
        return vec![format!("{} {}", style("http").green().bold(), daemon_url)];
    }

    let mut lines = Vec::new();
    if has_metadata_warning(inspection) {
        lines.push(format!(
            "{} daemon metadata is missing or stale for this resolved home.",
            style("warning").yellow().bold()
        ));
        if inspection.process.is_some() {
            lines.push(format!(
                "cleanup: {}",
                daemon_stop_command(&inspection.home_dir)
            ));
        } else {
            lines.push(
                "cleanup: metadata is unavailable; manually confirm any listener or pid before terminating it".to_string(),
            );
            lines.push("check: lsof -nP -iTCP:<port> -sTCP:LISTEN".to_string());
        }
    }

    lines.extend([
        format!(
            "{} daemon is not running for this resolved home.",
            style("note").yellow().bold()
        ),
        format!("home: {}", inspection.home_dir.display()),
        format!("daemon_url: {daemon_url}"),
        format!("start: {}", daemon_start_command(&inspection.home_dir)),
    ]);
    lines
}

fn has_metadata_warning(inspection: &DaemonInspection) -> bool {
    inspection.warnings.iter().any(|warning| {
        matches!(
            warning.code.as_str(),
            "runtime_home_missing"
                | "runtime_dir_missing"
                | "process_path_missing"
                | "pid_path_stale"
                | "process_metadata_stale"
        )
    })
}

fn render_daemon_warnings(warnings: &[DaemonWarning]) {
    if warnings.is_empty() {
        return;
    }
    println!("{} Daemon warnings", style("warning").yellow().bold());
    for warning in warnings {
        println!(
            "{} {}",
            style(&warning.code).yellow().bold(),
            warning.message
        );
        if let Some(path) = &warning.path {
            println!("path: {}", path.display());
        }
    }
}

fn daemon_start_command(home_dir: &Path) -> String {
    format!(
        "tentgent daemon start --home {} --host {} --port {}",
        shell_single_quote(home_dir),
        DEFAULT_DAEMON_HOST,
        DEFAULT_DAEMON_PORT
    )
}

fn daemon_stop_command(home_dir: &Path) -> String {
    format!(
        "tentgent daemon stop --home {}",
        shell_single_quote(home_dir)
    )
}

fn shell_single_quote(path: &Path) -> String {
    let value = path.display().to_string();
    format!("'{}'", value.replace('\'', "'\\''"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_and_run_detach_share_detached_options() {
        let home = PathBuf::from("/tmp/tentgent-home");
        let run = DetachedDaemonOptions::from_run(DaemonRunCommand {
            home: Some(home.clone()),
            host: Some("127.0.0.1".to_string()),
            port: Some(8791),
            allow_unsafe_bind: true,
            detach: true,
        });
        let start = DetachedDaemonOptions::from_start(DaemonStartCommand {
            home: Some(home),
            host: Some("127.0.0.1".to_string()),
            port: Some(8791),
            allow_unsafe_bind: true,
        });

        assert_eq!(run, start);
    }

    #[test]
    fn stopped_daemon_guidance_includes_home_url_and_start_command() {
        let inspection = stopped_inspection("/tmp/tentgent-home");
        let lines = daemon_guidance_lines(&inspection);

        assert!(lines
            .iter()
            .any(|line| line.contains("daemon is not running for this resolved home")));
        assert!(lines.iter().any(|line| line == "home: /tmp/tentgent-home"));
        assert!(lines.iter().any(|line| line.starts_with("daemon_url: ")));
        assert!(lines.iter().any(|line| {
            line == "start: tentgent daemon start --home '/tmp/tentgent-home' --host 127.0.0.1 --port 8790"
        }));
    }

    #[test]
    fn daemon_warning_guidance_uses_manual_cleanup_when_metadata_is_missing() {
        let mut inspection = stopped_inspection("/tmp/tentgent-missing-home");
        inspection.warnings.push(DaemonWarning {
            code: "runtime_home_missing".to_string(),
            message: "runtime home is missing".to_string(),
            path: Some(PathBuf::from("/tmp/tentgent-missing-home")),
        });
        let lines = daemon_guidance_lines(&inspection);

        assert!(lines.iter().any(|line| {
            line.contains("daemon metadata is missing or stale for this resolved home")
        }));
        assert!(lines.iter().any(|line| {
            line.contains("manually confirm any listener or pid before terminating it")
        }));
        assert!(!lines
            .iter()
            .any(|line| line.starts_with("cleanup: tentgent daemon stop")));
    }

    fn stopped_inspection(home: &str) -> DaemonInspection {
        inspection(home, false)
    }

    fn inspection(home: &str, running: bool) -> DaemonInspection {
        let home = PathBuf::from(home);
        DaemonInspection {
            home_dir: home.clone(),
            runtime_dir: home.join("runtime"),
            log_dir: home.join("logs"),
            process_path: home.join("runtime/daemon.toml"),
            pid_path: home.join("runtime/tentgent.pid"),
            stdout_log_path: home.join("logs/daemon.stdout.log"),
            stderr_log_path: home.join("logs/daemon.stderr.log"),
            running,
            process: None,
            warnings: Vec::new(),
        }
    }
}
