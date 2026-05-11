use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    platform::{current_backend_capabilities, BackendCapabilityState, PlatformInfo},
    runtime_assets::{
        resolve_bootstrap_cache_dir, resolve_runtime_home, PythonRuntime, PythonRuntimeSource,
    },
    VERSION,
};

const STANDARD_DIRS: &[&str] = &[
    "models", "servers", "adapters", "datasets", "train", "cache", "runtime", "logs", "locks",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCheckStatus {
    Pass,
    Warn,
    Fail,
    Skipped,
}

impl DoctorCheckStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorCheckStatus,
    pub detail: String,
}

impl DoctorCheck {
    fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Pass,
            detail: detail.into(),
        }
    }

    fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DoctorCheckStatus::Fail,
            detail: detail.into(),
        }
    }

    fn with_status(
        name: impl Into<String>,
        status: DoctorCheckStatus,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoctorSummary {
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorOptions {
    pub observational: bool,
    pub runtime_home: Option<PathBuf>,
}

impl DoctorOptions {
    pub fn observational() -> Self {
        Self {
            observational: true,
            runtime_home: None,
        }
    }

    pub fn cli() -> Self {
        Self {
            observational: false,
            runtime_home: None,
        }
    }

    pub fn with_runtime_home(mut self, runtime_home: PathBuf) -> Self {
        self.runtime_home = Some(runtime_home);
        self
    }
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub status: DoctorCheckStatus,
    pub summary: DoctorSummary,
    pub checks: Vec<DoctorCheck>,
}

pub fn build_doctor_report(options: DoctorOptions) -> DoctorReport {
    let mut checks = Vec::new();

    checks.push(DoctorCheck::pass("cli version", VERSION));

    let platform = PlatformInfo::current();
    checks.push(DoctorCheck::pass("platform", platform.label()));

    let mut uv = check_command("uv", &["--version"], DoctorCheckStatus::Warn);
    uv.name = "uv dev bootstrap".to_string();
    uv.detail = match uv.status {
        DoctorCheckStatus::Pass => {
            format!("available for current developer bootstrap: {}", uv.detail)
        }
        DoctorCheckStatus::Warn | DoctorCheckStatus::Fail | DoctorCheckStatus::Skipped => format!(
            "needed only by the current developer bootstrap; release installers must bundle or replace this step: {}",
            uv.detail
        ),
    };
    let uv_available = uv.status == DoctorCheckStatus::Pass;

    match options
        .runtime_home
        .clone()
        .map(Ok)
        .unwrap_or_else(resolve_runtime_home)
    {
        Ok(runtime_home) => {
            checks.push(check_directory("runtime home", &runtime_home, &options));
            for name in STANDARD_DIRS {
                checks.push(check_directory(
                    format!("dir {name}"),
                    &runtime_home.join(name),
                    &options,
                ));
            }
        }
        Err(err) => checks.push(DoctorCheck::fail(
            "runtime home",
            format!("could not resolve runtime home: {err}"),
        )),
    }
    match resolve_bootstrap_cache_dir() {
        Ok(path) => checks.push(check_optional_directory("bootstrap cache", &path, &options)),
        Err(err) => checks.push(DoctorCheck::with_status(
            "bootstrap cache",
            DoctorCheckStatus::Warn,
            format!("could not resolve bootstrap cache: {err}"),
        )),
    }

    match PythonRuntime::resolve() {
        Ok(runtime) => {
            let bootstrap_status = match runtime.source() {
                PythonRuntimeSource::InstalledPrefix => DoctorCheckStatus::Fail,
                PythonRuntimeSource::DevelopmentSource
                | PythonRuntimeSource::EnvironmentOverride => {
                    if uv_available {
                        DoctorCheckStatus::Warn
                    } else {
                        DoctorCheckStatus::Fail
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
            BackendCapabilityState::Enabled => DoctorCheckStatus::Pass,
            BackendCapabilityState::DependencyGated | BackendCapabilityState::Unsupported => {
                DoctorCheckStatus::Warn
            }
        };
        checks.push(DoctorCheck {
            name: format!("backend {}", capability.backend.as_str()),
            status,
            detail: capability.summary(),
        });
    }

    let summary = summarize_checks(&checks);
    let status = if summary.fail > 0 {
        DoctorCheckStatus::Fail
    } else if summary.warn > 0 {
        DoctorCheckStatus::Warn
    } else {
        DoctorCheckStatus::Pass
    };

    DoctorReport {
        status,
        summary,
        checks,
    }
}

fn summarize_checks(checks: &[DoctorCheck]) -> DoctorSummary {
    let mut summary = DoctorSummary {
        pass: 0,
        warn: 0,
        fail: 0,
        skipped: 0,
    };
    for check in checks {
        match check.status {
            DoctorCheckStatus::Pass => summary.pass += 1,
            DoctorCheckStatus::Warn => summary.warn += 1,
            DoctorCheckStatus::Fail => summary.fail += 1,
            DoctorCheckStatus::Skipped => summary.skipped += 1,
        }
    }
    summary
}

fn check_directory(name: impl Into<String>, path: &Path, options: &DoctorOptions) -> DoctorCheck {
    let name = name.into();
    if path.exists() {
        if !path.is_dir() {
            return DoctorCheck::fail(name, format!("not a directory: {}", path.display()));
        }
        if options.observational {
            return DoctorCheck::pass(name, format!("present: {}", path.display()));
        }
        return match write_probe(path) {
            Ok(()) => DoctorCheck::pass(name, format!("writable: {}", path.display())),
            Err(err) => DoctorCheck::fail(name, format!("not writable: {}; {err}", path.display())),
        };
    }

    if options.observational {
        return DoctorCheck::with_status(
            name,
            DoctorCheckStatus::Warn,
            format!(
                "missing: {}; observational doctor did not create it",
                path.display()
            ),
        );
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

fn check_optional_directory(
    name: impl Into<String>,
    path: &Path,
    options: &DoctorOptions,
) -> DoctorCheck {
    let name = name.into();
    if path.exists() {
        if !path.is_dir() {
            return DoctorCheck::with_status(
                name,
                DoctorCheckStatus::Warn,
                format!(
                    "optional path exists but is not a directory: {}",
                    path.display()
                ),
            );
        }
        if options.observational {
            return DoctorCheck::pass(name, format!("present: {}", path.display()));
        }
        return match write_probe(path) {
            Ok(()) => DoctorCheck::pass(name, format!("present and writable: {}", path.display())),
            Err(err) => DoctorCheck::with_status(
                name,
                DoctorCheckStatus::Warn,
                format!("optional path is not writable: {}; {err}", path.display()),
            ),
        };
    }

    if options.observational {
        return DoctorCheck::with_status(
            name,
            DoctorCheckStatus::Skipped,
            format!("optional path is missing: {}", path.display()),
        );
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
            DoctorCheckStatus::Warn,
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
    missing_status: DoctorCheckStatus,
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
    missing_status: DoctorCheckStatus,
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
    missing_status: DoctorCheckStatus,
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
            "run `tentgent runtime bootstrap`, then run `tentgent doctor` again"
        }
        PythonRuntimeSource::DevelopmentSource | PythonRuntimeSource::EnvironmentOverride => {
            "run `tentgent doctor --fix` during development or `tentgent runtime bootstrap` for packaged installs"
        }
    }
}

fn check_command(name: &str, args: &[&str], missing_status: DoctorCheckStatus) -> DoctorCheck {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_aggregation_prefers_fail_then_warn() {
        let report = DoctorReport {
            status: DoctorCheckStatus::Pass,
            summary: summarize_checks(&[
                DoctorCheck::pass("a", "ok"),
                DoctorCheck::with_status("b", DoctorCheckStatus::Warn, "warn"),
                DoctorCheck::fail("c", "fail"),
                DoctorCheck::with_status("d", DoctorCheckStatus::Skipped, "skip"),
            ]),
            checks: Vec::new(),
        };

        assert_eq!(report.summary.pass, 1);
        assert_eq!(report.summary.warn, 1);
        assert_eq!(report.summary.fail, 1);
        assert_eq!(report.summary.skipped, 1);
    }

    #[test]
    fn observational_directory_check_does_not_write_probe() {
        let missing = std::env::temp_dir().join(format!(
            "tentgent-doctor-observational-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&missing);

        let check = check_directory(
            "missing",
            &missing,
            &DoctorOptions {
                observational: true,
                runtime_home: None,
            },
        );

        assert_eq!(check.status, DoctorCheckStatus::Warn);
        assert!(!missing.exists());
    }
}
