use std::{
    env,
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use reqwest::StatusCode;
use tentgent_core::daemon::{
    DaemonError, DaemonInspection, DaemonManager, DaemonRunRequest, DaemonRunSpec,
    DaemonStopOutcome, DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT,
};
use tentgent_http::{
    security::{check_bind_safety, BindSafetyReport, DaemonSecurityConfig, DAEMON_TOKEN_ENV_VAR},
    DaemonHttpServer, DaemonHttpState,
};
use tokio::time::{sleep, Instant};

use super::commands::{DaemonCommands, DaemonRunCommand, DaemonStartCommand};

const DAEMON_URL_ENV_VAR: &str = "TENTGENT_DAEMON_URL";
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const DAEMON_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DAEMON_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

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
            let manager = DaemonManager::new(home.as_deref()).into_diagnostic()?;
            let inspection = manager.status().into_diagnostic()?;
            render_daemon_inspection("Daemon status", &inspection);
            render_daemon_guidance(&inspection);
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
    let bind_safety =
        validate_daemon_bind_safety(&spec.host, &security, command.allow_unsafe_bind)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetachedDaemonChildCommand {
    executable: PathBuf,
    args: Vec<String>,
    stdout_log_path: PathBuf,
    stderr_log_path: PathBuf,
}

#[derive(Debug)]
struct DaemonReadiness {
    inspection: DaemonInspection,
    status_warning: Option<String>,
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
    let manager = DaemonManager::new(options.home.as_deref()).into_diagnostic()?;
    let initial = manager.status().into_diagnostic()?;
    if initial.running {
        return existing_daemon_outcome(&initial).await;
    }

    let spec = match manager.prepare_run(DaemonRunRequest {
        host: options.host,
        port: options.port,
    }) {
        Ok(spec) => spec,
        Err(DaemonError::AlreadyRunning(_)) => {
            let inspection = manager.status().into_diagnostic()?;
            return existing_daemon_outcome(&inspection).await;
        }
        Err(error) => return Err(error).into_diagnostic(),
    };

    let security = DaemonSecurityConfig::from_env();
    let bind_safety =
        validate_daemon_bind_safety(&spec.host, &security, options.allow_unsafe_bind)?;
    let bind_warnings = bind_safety.warnings;

    let child = build_detached_child_command(
        env::current_exe().into_diagnostic()?,
        &spec,
        options.allow_unsafe_bind,
    );
    let launched_pid = launch_detached_child(&child).await?;
    let expected_url = daemon_url(&spec.host, spec.port);
    let token = read_daemon_token_from_env();
    let readiness = wait_for_daemon_readiness(
        &manager,
        &expected_url,
        &spec.inspection.home_dir,
        &child.stdout_log_path,
        &child.stderr_log_path,
        token.as_deref(),
        DAEMON_STARTUP_TIMEOUT,
    )
    .await?;

    Ok(DetachedDaemonStartOutcome {
        daemon_url: daemon_url_from_process_metadata(&readiness.inspection).unwrap_or(expected_url),
        inspection: readiness.inspection,
        launched_pid,
        stdout_log_path: child.stdout_log_path,
        stderr_log_path: child.stderr_log_path,
        status_warning: readiness.status_warning,
        bind_warnings,
        already_running: false,
    })
}

async fn existing_daemon_outcome(
    inspection: &DaemonInspection,
) -> miette::Result<DetachedDaemonStartOutcome> {
    let process = inspection.process.as_ref().ok_or_else(|| {
        miette!("daemon metadata was expected to be running but no process metadata was present")
    })?;
    let url = daemon_url(&process.host, process.port);
    let client = probe_client()?;
    match probe_healthz(&client, &url).await {
        Ok(()) => Ok(DetachedDaemonStartOutcome {
            inspection: inspection.clone(),
            daemon_url: url,
            launched_pid: None,
            stdout_log_path: inspection.stdout_log_path.clone(),
            stderr_log_path: inspection.stderr_log_path.clone(),
            status_warning: None,
            bind_warnings: Vec::new(),
            already_running: true,
        }),
        Err(detail) => Err(existing_daemon_unreachable_error(inspection, &url, &detail)),
    }
}

fn validate_daemon_bind_safety(
    host: &str,
    security: &DaemonSecurityConfig,
    allow_unsafe_bind: bool,
) -> miette::Result<BindSafetyReport> {
    check_bind_safety(host, security.token_enabled(), allow_unsafe_bind)
}

fn build_detached_child_command(
    executable: PathBuf,
    spec: &DaemonRunSpec,
    allow_unsafe_bind: bool,
) -> DetachedDaemonChildCommand {
    let mut args = vec![
        "daemon".to_string(),
        "run".to_string(),
        "--home".to_string(),
        spec.inspection.home_dir.display().to_string(),
        "--host".to_string(),
        spec.host.clone(),
        "--port".to_string(),
        spec.port.to_string(),
    ];
    if allow_unsafe_bind {
        args.push("--allow-unsafe-bind".to_string());
    }

    DetachedDaemonChildCommand {
        executable,
        args,
        stdout_log_path: spec.inspection.stdout_log_path.clone(),
        stderr_log_path: spec.inspection.stderr_log_path.clone(),
    }
}

async fn launch_detached_child(
    command: &DetachedDaemonChildCommand,
) -> miette::Result<Option<u32>> {
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&command.stdout_log_path)
        .into_diagnostic()?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&command.stderr_log_path)
        .into_diagnostic()?;

    let mut child = Command::new(&command.executable);
    child
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    {
        child.process_group(0);
    }

    let child = child.spawn().into_diagnostic()?;
    Ok(Some(child.id()))
}

async fn wait_for_daemon_readiness(
    manager: &DaemonManager,
    expected_url: &str,
    home_dir: &Path,
    stdout_log_path: &Path,
    stderr_log_path: &Path,
    token: Option<&str>,
    wait_duration: Duration,
) -> miette::Result<DaemonReadiness> {
    let client = probe_client()?;
    let deadline = Instant::now() + wait_duration;
    let mut last_probe_error: Option<String> = None;

    loop {
        let inspection = manager.status().into_diagnostic()?;
        if let Some(process) = inspection.process.as_ref() {
            if inspection.running {
                let url = daemon_url(&process.host, process.port);
                match probe_healthz(&client, &url).await {
                    Ok(()) => {
                        let status_warning = match token {
                            Some(token) => probe_status_warning(&client, &url, token).await,
                            None => None,
                        };
                        return Ok(DaemonReadiness {
                            inspection,
                            status_warning,
                        });
                    }
                    Err(detail) => {
                        last_probe_error = Some(detail);
                    }
                }
            }
        }

        if Instant::now() >= deadline {
            return Err(startup_timeout_error(
                expected_url,
                home_dir,
                stdout_log_path,
                stderr_log_path,
                last_probe_error.as_deref(),
            ));
        }

        sleep(DAEMON_READINESS_POLL_INTERVAL).await;
    }
}

fn probe_client() -> miette::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(DAEMON_PROBE_TIMEOUT)
        .build()
        .into_diagnostic()
}

async fn probe_healthz(client: &reqwest::Client, daemon_url: &str) -> Result<(), String> {
    let response = client
        .get(endpoint_url(daemon_url, "/healthz"))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("GET /healthz returned {}", response.status()))
    }
}

async fn probe_status_warning(
    client: &reqwest::Client,
    daemon_url: &str,
    token: &str,
) -> Option<String> {
    let response = client
        .get(endpoint_url(daemon_url, "/v1/status"))
        .bearer_auth(token)
        .send()
        .await;
    match response {
        Ok(response) => status_probe_warning(response.status()),
        Err(error) => Some(format!(
            "daemon ready but /v1/status could not be confirmed: {error}"
        )),
    }
}

fn status_probe_warning(status: StatusCode) -> Option<String> {
    if status.is_success() {
        None
    } else if status == StatusCode::UNAUTHORIZED {
        Some("daemon ready but status requires a valid token".to_string())
    } else {
        Some(format!("daemon ready but /v1/status returned {status}"))
    }
}

fn read_daemon_token_from_env() -> Option<String> {
    env::var(DAEMON_TOKEN_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn endpoint_url(daemon_url: &str, path: &str) -> String {
    format!("{}{}", daemon_url.trim_end_matches('/'), path)
}

fn daemon_url(host: &str, port: u16) -> String {
    format!("http://{}:{port}", host_for_url(host))
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

fn host_for_url(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed.to_string()
    } else if trimmed.contains(':') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    }
}

fn startup_timeout_error(
    expected_url: &str,
    home_dir: &Path,
    stdout_log_path: &Path,
    stderr_log_path: &Path,
    last_probe_error: Option<&str>,
) -> miette::Report {
    let detail = last_probe_error
        .map(|detail| format!("\nlast health probe: {detail}"))
        .unwrap_or_default();
    miette!(
        "timed out waiting for daemon readiness at {expected_url}\nhome: {}\nstdout log: {}\nstderr log: {}{detail}",
        home_dir.display(),
        stdout_log_path.display(),
        stderr_log_path.display(),
    )
}

fn existing_daemon_unreachable_error(
    inspection: &DaemonInspection,
    url: &str,
    detail: &str,
) -> miette::Report {
    let process = inspection
        .process
        .as_ref()
        .expect("caller provides running daemon inspection");
    miette!(
        "daemon metadata under this home points to live pid {}, but {url}/healthz is not reachable: {detail}\nhome: {}\nstdout log: {}\nstderr log: {}\nstop command: {}",
        process.pid,
        inspection.home_dir.display(),
        inspection.stdout_log_path.display(),
        inspection.stderr_log_path.display(),
        daemon_stop_command(&inspection.home_dir),
    )
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

    vec![
        format!(
            "{} daemon is not running for this resolved home.",
            style("note").yellow().bold()
        ),
        format!("home: {}", inspection.home_dir.display()),
        format!("daemon_url: {daemon_url}"),
        format!("start: {}", daemon_start_command(&inspection.home_dir)),
    ]
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
    fn detached_child_command_reuses_foreground_run_without_detach() {
        let spec = daemon_run_spec("/tmp/tentgent-home", "127.0.0.1", 8791);
        let command = build_detached_child_command(PathBuf::from("/bin/tentgent"), &spec, true);

        assert_eq!(command.executable, PathBuf::from("/bin/tentgent"));
        assert_eq!(
            command.args,
            vec![
                "daemon",
                "run",
                "--home",
                "/tmp/tentgent-home",
                "--host",
                "127.0.0.1",
                "--port",
                "8791",
                "--allow-unsafe-bind",
            ]
        );
        assert!(!command.args.iter().any(|arg| arg == "--detach"));
        assert_eq!(
            command.stdout_log_path,
            PathBuf::from("/tmp/tentgent-home/logs/daemon.stdout.log")
        );
        assert_eq!(
            command.stderr_log_path,
            PathBuf::from("/tmp/tentgent-home/logs/daemon.stderr.log")
        );
    }

    #[test]
    fn daemon_bind_safety_helper_is_shared_for_detached_and_foreground_paths() {
        let disabled = DaemonSecurityConfig::disabled();
        assert!(validate_daemon_bind_safety("0.0.0.0", &disabled, false).is_err());
        assert!(validate_daemon_bind_safety("0.0.0.0", &disabled, true).is_ok());

        let enabled = DaemonSecurityConfig::from_token_value(Some("secret"));
        assert!(validate_daemon_bind_safety("0.0.0.0", &enabled, false).is_ok());
    }

    #[test]
    fn status_probe_unauthorized_is_warning_not_failure() {
        assert_eq!(
            status_probe_warning(StatusCode::UNAUTHORIZED).as_deref(),
            Some("daemon ready but status requires a valid token")
        );
        assert!(status_probe_warning(StatusCode::OK).is_none());
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

    fn daemon_run_spec(home: &str, host: &str, port: u16) -> DaemonRunSpec {
        DaemonRunSpec {
            host: host.to_string(),
            port,
            inspection: stopped_inspection(home),
        }
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
        }
    }
}
