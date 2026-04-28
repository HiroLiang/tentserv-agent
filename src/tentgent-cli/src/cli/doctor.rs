use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use console::style;
use miette::{miette, Result};
use tentgent_core::{
    platform::{current_backend_capabilities, BackendCapabilityState, PlatformInfo},
    runtime_assets::{
        resolve_bootstrap_cache_dir, resolve_runtime_home, PythonRuntime, PythonRuntimeSource,
    },
};

use super::commands::DoctorCommand;

const STANDARD_DIRS: &[&str] = &[
    "models", "servers", "adapters", "datasets", "train", "cache", "runtime", "logs", "locks",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug)]
struct DoctorCheck {
    name: String,
    status: CheckStatus,
    detail: String,
}

impl DoctorCheck {
    fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }

    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }

    fn with_status(
        name: impl Into<String>,
        status: CheckStatus,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
        }
    }
}

pub fn handle_doctor_command(command: DoctorCommand) -> Result<()> {
    if command.fix {
        bootstrap_python_env()?;
    }

    let mut checks = Vec::new();

    checks.push(DoctorCheck::pass("cli version", env!("CARGO_PKG_VERSION")));

    let platform = PlatformInfo::current();
    checks.push(DoctorCheck::pass("platform", platform.label()));

    let mut uv = check_command("uv", &["--version"], CheckStatus::Warn);
    uv.name = "uv dev bootstrap".to_string();
    uv.detail = match uv.status {
        CheckStatus::Pass => format!("available for current developer bootstrap: {}", uv.detail),
        CheckStatus::Warn | CheckStatus::Fail => format!(
            "needed only by the current developer bootstrap; release installers must bundle or replace this step: {}",
            uv.detail
        ),
    };
    let uv_available = uv.status == CheckStatus::Pass;

    match resolve_runtime_home() {
        Ok(runtime_home) => {
            checks.push(check_directory("runtime home", &runtime_home));
            for name in STANDARD_DIRS {
                checks.push(check_directory(
                    format!("dir {name}"),
                    &runtime_home.join(name),
                ));
            }
        }
        Err(err) => checks.push(DoctorCheck::fail(
            "runtime home",
            format!("could not resolve runtime home: {err}"),
        )),
    }
    match resolve_bootstrap_cache_dir() {
        Ok(path) => checks.push(check_optional_directory("bootstrap cache", &path)),
        Err(err) => checks.push(DoctorCheck::with_status(
            "bootstrap cache",
            CheckStatus::Warn,
            format!("could not resolve bootstrap cache: {err}"),
        )),
    }

    match PythonRuntime::resolve() {
        Ok(runtime) => {
            let bootstrap_status = match runtime.source() {
                PythonRuntimeSource::InstalledPrefix => CheckStatus::Fail,
                PythonRuntimeSource::DevelopmentSource
                | PythonRuntimeSource::EnvironmentOverride => {
                    if uv_available {
                        CheckStatus::Warn
                    } else {
                        CheckStatus::Fail
                    }
                }
            };
            checks.push(DoctorCheck::pass(
                "python source",
                runtime.source().as_str(),
            ));
            checks.push(check_file("python pyproject", &runtime.pyproject_path()));
            checks.push(check_directory_present(
                "python package",
                &runtime.python_src_dir(),
            ));
            checks.push(check_python_env(
                runtime.env_dir(),
                runtime.source(),
                bootstrap_status,
            ));
            checks.push(check_python_file(
                "python binary",
                &runtime.python_bin(),
                runtime.source(),
                bootstrap_status,
            ));
            checks.push(check_python_version(
                &runtime.python_bin(),
                runtime.source(),
                bootstrap_status,
            ));
            for script in [
                "tentgent-chat-once",
                "tentgent-server",
                "tentgent-train-lora-run",
                "tentgent-hf-snapshot",
            ] {
                checks.push(check_python_file(
                    format!("entrypoint {script}"),
                    &runtime.script_bin(script),
                    runtime.source(),
                    bootstrap_status,
                ));
            }
        }
        Err(err) => checks.push(DoctorCheck::fail(
            "python runtime",
            format!("could not resolve Python runtime assets: {err}"),
        )),
    }

    checks.push(uv);

    for capability in current_backend_capabilities() {
        let status = match capability.state {
            BackendCapabilityState::Enabled => CheckStatus::Pass,
            BackendCapabilityState::DependencyGated | BackendCapabilityState::Unsupported => {
                CheckStatus::Warn
            }
        };
        checks.push(DoctorCheck {
            name: format!("backend {}", capability.backend.as_str()),
            status,
            detail: capability.summary(),
        });
    }

    render_checks(&checks);

    let failures = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Fail)
        .count();
    if failures > 0 {
        return Err(miette!("doctor found {failures} failure(s)"));
    }

    Ok(())
}

fn bootstrap_python_env() -> Result<()> {
    let runtime = PythonRuntime::resolve()
        .map_err(|err| miette!("failed to resolve Python runtime assets: {err}"))?;
    let parent = runtime
        .env_dir()
        .parent()
        .ok_or_else(|| miette!("failed to resolve parent directory for Python env"))?;
    fs::create_dir_all(parent).map_err(|err| {
        miette!(
            "failed to create Python env parent `{}`: {err}",
            parent.display()
        )
    })?;

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Developer Python environment bootstrap").bold()
    );
    println!("project: {}", runtime.project_dir().display());
    println!("env: {}", runtime.env_dir().display());

    let mut process = Command::new("uv");
    process
        .current_dir(runtime.project_dir())
        .env("UV_PROJECT_ENVIRONMENT", runtime.env_dir())
        .arg("--no-config")
        .arg("sync")
        .arg("--project")
        .arg(runtime.project_dir())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = process
        .status()
        .map_err(|err| miette!("failed to run uv for developer Python env bootstrap: {err}"))?;
    if !status.success() {
        return Err(miette!(
            "developer Python env bootstrap failed with status {status}"
        ));
    }

    println!();
    Ok(())
}

fn render_checks(checks: &[DoctorCheck]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent doctor").bold()
    );

    for check in checks {
        println!(
            "{} {:<34} {}",
            status_marker(check.status),
            check.name,
            short_summary(&check.detail)
        );
    }
    render_details(checks);

    let failures = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Fail)
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Warn)
        .count();
    let result = if failures > 0 {
        format!("blocked with {failures} failure(s) and {warnings} warning(s)")
    } else if warnings > 0 {
        format!("ready with {warnings} warning(s)")
    } else {
        "ready".to_string()
    };

    println!("Result: {result}");
    println!();
}

fn render_details(checks: &[DoctorCheck]) {
    let notable = checks
        .iter()
        .filter(|check| check.status != CheckStatus::Pass || should_show_detail(check))
        .collect::<Vec<_>>();
    if notable.is_empty() {
        return;
    }

    println!();
    println!("{}", style("Details").bold());
    for check in notable {
        println!(
            "{} {}: {}",
            status_marker(check.status),
            style(&check.name).bold(),
            check.detail
        );
    }
}

fn should_show_detail(check: &DoctorCheck) -> bool {
    matches!(
        check.name.as_str(),
        "runtime home"
            | "bootstrap cache"
            | "python source"
            | "python pyproject"
            | "python env"
            | "python binary"
    )
}

fn short_summary(detail: &str) -> String {
    let summary = detail.split(':').next().unwrap_or(detail).trim();
    truncate_middle(summary, 42)
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }

    let head_len = max_chars.saturating_sub(1) / 2;
    let tail_len = max_chars.saturating_sub(1 + head_len);
    let head = value.chars().take(head_len).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}…{tail}")
}

fn status_marker(status: CheckStatus) -> console::StyledObject<&'static str> {
    match status {
        CheckStatus::Pass => style("ok").green().bold(),
        CheckStatus::Warn => style("warn").yellow().bold(),
        CheckStatus::Fail => style("fail").red().bold(),
    }
}

fn check_directory(name: impl Into<String>, path: &Path) -> DoctorCheck {
    let name = name.into();
    if path.exists() {
        if !path.is_dir() {
            return DoctorCheck::fail(name, format!("not a directory: {}", path.display()));
        }
        return match write_probe(path) {
            Ok(()) => DoctorCheck::pass(name, format!("writable: {}", path.display())),
            Err(err) => DoctorCheck::fail(name, format!("not writable: {}; {err}", path.display())),
        };
    }

    match nearest_existing_parent(path).and_then(|parent| write_probe(&parent).map(|_| parent)) {
        Ok(parent) => DoctorCheck::pass(
            name,
            format!(
                "creatable on demand; parent is writable: {}",
                parent.display()
            ),
        ),
        Err(err) => DoctorCheck::fail(
            name,
            format!(
                "missing and cannot verify creation: {}; {err}",
                path.display()
            ),
        ),
    }
}

fn check_optional_directory(name: impl Into<String>, path: &Path) -> DoctorCheck {
    let name = name.into();
    if path.exists() {
        if !path.is_dir() {
            return DoctorCheck::with_status(
                name,
                CheckStatus::Warn,
                format!(
                    "optional path exists but is not a directory: {}",
                    path.display()
                ),
            );
        }
        return match write_probe(path) {
            Ok(()) => DoctorCheck::pass(name, format!("present and writable: {}", path.display())),
            Err(err) => DoctorCheck::with_status(
                name,
                CheckStatus::Warn,
                format!("optional path is not writable: {}; {err}", path.display()),
            ),
        };
    }

    match nearest_existing_parent(path).and_then(|parent| write_probe(&parent).map(|_| parent)) {
        Ok(parent) => DoctorCheck::pass(
            name,
            format!(
                "optional; installer can create it on demand; parent is writable: {}",
                parent.display()
            ),
        ),
        Err(err) => DoctorCheck::with_status(
            name,
            CheckStatus::Warn,
            format!(
                "optional but installer may not be able to create it: {}; {err}",
                path.display()
            ),
        ),
    }
}

fn check_directory_present(name: impl Into<String>, path: &Path) -> DoctorCheck {
    let name = name.into();
    if path.is_dir() {
        DoctorCheck::pass(name, format!("present: {}", path.display()))
    } else {
        DoctorCheck::fail(name, format!("missing: {}", path.display()))
    }
}

fn check_python_env(
    path: &Path,
    source: PythonRuntimeSource,
    missing_status: CheckStatus,
) -> DoctorCheck {
    if path.is_dir() {
        DoctorCheck::pass("python env", format!("present: {}", path.display()))
    } else {
        DoctorCheck::with_status(
            "python env",
            missing_status,
            format!(
                "missing: {}; {}",
                path.display(),
                python_bootstrap_hint(source)
            ),
        )
    }
}

fn check_python_file(
    name: impl Into<String>,
    path: &Path,
    source: PythonRuntimeSource,
    missing_status: CheckStatus,
) -> DoctorCheck {
    let name = name.into();
    if path.is_file() {
        DoctorCheck::pass(name, format!("present: {}", path.display()))
    } else {
        DoctorCheck::with_status(
            name,
            missing_status,
            format!(
                "missing: {}; {}",
                path.display(),
                python_bootstrap_hint(source)
            ),
        )
    }
}

fn check_file(name: impl Into<String>, path: &Path) -> DoctorCheck {
    let name = name.into();
    if path.is_file() {
        DoctorCheck::pass(name, format!("present: {}", path.display()))
    } else {
        DoctorCheck::fail(name, format!("missing: {}", path.display()))
    }
}

fn check_python_version(
    path: &Path,
    source: PythonRuntimeSource,
    missing_status: CheckStatus,
) -> DoctorCheck {
    if !path.is_file() {
        return DoctorCheck::with_status(
            "python version",
            missing_status,
            format!(
                "python binary is missing; {}",
                python_bootstrap_hint(source)
            ),
        );
    }
    match Command::new(path).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let version = if stdout.trim().is_empty() {
                stderr.trim()
            } else {
                stdout.trim()
            };
            DoctorCheck::pass("python version", version)
        }
        Ok(output) => DoctorCheck::fail(
            "python version",
            format!("python exited with status {}", output.status),
        ),
        Err(err) => DoctorCheck::fail("python version", err.to_string()),
    }
}

fn python_bootstrap_hint(source: PythonRuntimeSource) -> &'static str {
    match source {
        PythonRuntimeSource::InstalledPrefix => {
            "run the installer Python bootstrap, then run `tentgent doctor` again"
        }
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            "run `tentgent doctor --fix` during development or use the installer Python bootstrap"
        }
    }
}

fn check_command(name: &str, args: &[&str], missing_status: CheckStatus) -> DoctorCheck {
    match Command::new(name).args(args).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = if stdout.trim().is_empty() {
                stderr.trim()
            } else {
                stdout.trim()
            };
            DoctorCheck::pass(name, detail)
        }
        Ok(output) => DoctorCheck {
            name: name.to_string(),
            status: missing_status,
            detail: format!("command exited with status {}", output.status),
        },
        Err(err) => DoctorCheck {
            name: name.to_string(),
            status: missing_status,
            detail: format!("not available on PATH: {err}"),
        },
    }
}

fn nearest_existing_parent(path: &Path) -> std::io::Result<PathBuf> {
    let mut cursor = path;
    loop {
        if cursor.exists() {
            return Ok(cursor.to_path_buf());
        }
        cursor = cursor
            .parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no parent"))?;
    }
}

fn write_probe(dir: &Path) -> std::io::Result<()> {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = dir.join(format!(
        ".tentgent-doctor-probe-{}-{id}",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(b"probe")?;
    drop(file);
    fs::remove_file(path)?;
    Ok(())
}
