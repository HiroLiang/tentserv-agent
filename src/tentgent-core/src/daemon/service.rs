use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use super::{
    error::DaemonError,
    store::{
        created_at_now, read_process_metadata, write_pid_file, write_process_metadata,
        DaemonProcessMetadata, DaemonStorePaths, DEFAULT_DAEMON_HOST, DEFAULT_DAEMON_PORT,
    },
};

#[derive(Debug, Clone)]
pub struct DaemonManager {
    paths: DaemonStorePaths,
}

#[derive(Debug, Clone)]
pub struct DaemonRunRequest {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct DaemonRunSpec {
    pub host: String,
    pub port: u16,
    pub inspection: DaemonInspection,
}

#[derive(Debug, Clone)]
pub struct DaemonInspection {
    pub home_dir: std::path::PathBuf,
    pub runtime_dir: std::path::PathBuf,
    pub log_dir: std::path::PathBuf,
    pub process_path: std::path::PathBuf,
    pub pid_path: std::path::PathBuf,
    pub stdout_log_path: std::path::PathBuf,
    pub stderr_log_path: std::path::PathBuf,
    pub running: bool,
    pub process: Option<DaemonProcessMetadata>,
    pub warnings: Vec<DaemonWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonWarning {
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DaemonStopOutcome {
    pub inspection: DaemonInspection,
    pub stopped_pid: u32,
}

impl DaemonManager {
    pub fn new(home_override: Option<&Path>) -> Result<Self, DaemonError> {
        let paths = DaemonStorePaths::resolve(home_override)?;
        paths.ensure_layout()?;
        Ok(Self { paths })
    }

    pub fn open_readonly(home_override: Option<&Path>) -> Result<Self, DaemonError> {
        let paths = DaemonStorePaths::resolve_without_create(home_override)?;
        Ok(Self { paths })
    }

    pub fn prepare_run(&self, request: DaemonRunRequest) -> Result<DaemonRunSpec, DaemonError> {
        let inspection = self.status()?;
        if let Some(process) = &inspection.process {
            if inspection.running {
                return Err(DaemonError::AlreadyRunning(process.pid));
            }
        }

        Ok(DaemonRunSpec {
            host: normalize_host(request.host.as_deref())?,
            port: request.port.unwrap_or(DEFAULT_DAEMON_PORT),
            inspection,
        })
    }

    pub fn status(&self) -> Result<DaemonInspection, DaemonError> {
        self.status_with_cleanup()
    }

    pub fn status_with_cleanup(&self) -> Result<DaemonInspection, DaemonError> {
        self.inspect(true)
    }

    pub fn status_observational(&self) -> Result<DaemonInspection, DaemonError> {
        self.inspect(false)
    }

    pub fn record_process_start(
        &self,
        pid: u32,
        host: String,
        port: u16,
    ) -> Result<DaemonInspection, DaemonError> {
        let inspection = self.status()?;
        if let Some(process) = &inspection.process {
            if inspection.running {
                return Err(DaemonError::AlreadyRunning(process.pid));
            }
        }

        let metadata = DaemonProcessMetadata {
            pid,
            host,
            port,
            started_at: created_at_now()?,
        };
        write_process_metadata(&self.paths.process_path, &metadata)?;
        write_pid_file(&self.paths.pid_path, pid)?;
        self.inspect(true)
    }

    pub fn clear_process_if_matches(&self, expected_pid: Option<u32>) -> Result<(), DaemonError> {
        if !self.paths.process_path.exists() {
            let _ = fs::remove_file(&self.paths.pid_path);
            return Ok(());
        }

        if let Some(expected_pid) = expected_pid {
            let current = read_process_metadata(&self.paths.process_path)?;
            if current.pid != expected_pid {
                return Ok(());
            }
        }

        let _ = fs::remove_file(&self.paths.process_path);
        let _ = fs::remove_file(&self.paths.pid_path);
        Ok(())
    }

    pub fn stop(&self) -> Result<DaemonStopOutcome, DaemonError> {
        let inspection = self.status()?;
        let process = inspection.process.clone().ok_or(DaemonError::NotRunning)?;
        if !inspection.running {
            return Err(DaemonError::NotRunning);
        }

        terminate_process(process.pid)?;
        self.clear_process_if_matches(Some(process.pid))?;
        let inspection = self.status()?;

        Ok(DaemonStopOutcome {
            inspection,
            stopped_pid: process.pid,
        })
    }

    fn inspect(&self, cleanup_stale: bool) -> Result<DaemonInspection, DaemonError> {
        let state = self.runtime_state(cleanup_stale)?;
        Ok(DaemonInspection {
            home_dir: self.paths.home_dir.clone(),
            runtime_dir: self.paths.runtime_dir.clone(),
            log_dir: self.paths.log_dir.clone(),
            process_path: self.paths.process_path.clone(),
            pid_path: self.paths.pid_path.clone(),
            stdout_log_path: self.paths.stdout_log_path.clone(),
            stderr_log_path: self.paths.stderr_log_path.clone(),
            running: state.running,
            process: state.process,
            warnings: state.warnings,
        })
    }

    fn runtime_state(&self, cleanup_stale: bool) -> Result<DaemonRuntimeState, DaemonError> {
        if !self.paths.home_dir.exists() {
            return Ok(DaemonRuntimeState::stopped(vec![daemon_warning(
                "runtime_home_missing",
                format!("runtime home is missing: {}", self.paths.home_dir.display()),
                Some(self.paths.home_dir.clone()),
            )]));
        }

        if !self.paths.runtime_dir.exists() {
            return Ok(DaemonRuntimeState::stopped(vec![daemon_warning(
                "runtime_dir_missing",
                format!(
                    "daemon runtime directory is missing: {}",
                    self.paths.runtime_dir.display()
                ),
                Some(self.paths.runtime_dir.clone()),
            )]));
        }

        if !self.paths.process_path.exists() {
            let warning = if self.paths.pid_path.exists() {
                daemon_warning(
                    "pid_path_stale",
                    format!(
                        "daemon pid file exists without process metadata: {}",
                        self.paths.pid_path.display()
                    ),
                    Some(self.paths.pid_path.clone()),
                )
            } else {
                daemon_warning(
                    "process_path_missing",
                    format!(
                        "daemon process metadata is missing: {}",
                        self.paths.process_path.display()
                    ),
                    Some(self.paths.process_path.clone()),
                )
            };
            return Ok(DaemonRuntimeState::stopped(vec![warning]));
        }

        let process = read_process_metadata(&self.paths.process_path)?;
        let running = is_process_running(process.pid)?;
        if running {
            let warnings = self.pid_file_warnings(&process);
            return Ok(DaemonRuntimeState {
                process: Some(process),
                running: true,
                warnings,
            });
        }

        let warning = daemon_warning(
            "process_metadata_stale",
            format!(
                "daemon metadata references pid {}, but that process is not running",
                process.pid
            ),
            Some(self.paths.process_path.clone()),
        );
        if cleanup_stale {
            let _ = fs::remove_file(&self.paths.process_path);
            let _ = fs::remove_file(&self.paths.pid_path);
            return Ok(DaemonRuntimeState::stopped(vec![warning]));
        }

        Ok(DaemonRuntimeState {
            process: Some(process),
            running: false,
            warnings: vec![warning],
        })
    }

    fn pid_file_warnings(&self, process: &DaemonProcessMetadata) -> Vec<DaemonWarning> {
        let Ok(pid_text) = fs::read_to_string(&self.paths.pid_path) else {
            return Vec::new();
        };
        let Ok(pid) = pid_text.trim().parse::<u32>() else {
            return vec![daemon_warning(
                "pid_path_stale",
                format!(
                    "daemon pid file is not a valid pid: {}",
                    self.paths.pid_path.display()
                ),
                Some(self.paths.pid_path.clone()),
            )];
        };
        if pid != process.pid {
            return vec![daemon_warning(
                "pid_path_stale",
                format!(
                    "daemon pid file records pid {pid}, but metadata records pid {}",
                    process.pid
                ),
                Some(self.paths.pid_path.clone()),
            )];
        }
        Vec::new()
    }
}

#[derive(Debug)]
struct DaemonRuntimeState {
    process: Option<DaemonProcessMetadata>,
    running: bool,
    warnings: Vec<DaemonWarning>,
}

impl DaemonRuntimeState {
    fn stopped(warnings: Vec<DaemonWarning>) -> Self {
        Self {
            process: None,
            running: false,
            warnings,
        }
    }
}

fn daemon_warning(
    code: impl Into<String>,
    message: impl Into<String>,
    path: Option<PathBuf>,
) -> DaemonWarning {
    DaemonWarning {
        code: code.into(),
        message: message.into(),
        path,
    }
}

fn normalize_host(value: Option<&str>) -> Result<String, DaemonError> {
    let host = value.unwrap_or(DEFAULT_DAEMON_HOST).trim();
    if host.is_empty() {
        return Err(DaemonError::EmptyHost);
    }

    Ok(host.to_string())
}

fn is_process_running(pid: u32) -> Result<bool, DaemonError> {
    let output = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Operation not permitted") || stderr.contains("operation not permitted") {
        return Ok(true);
    }

    Ok(false)
}

fn terminate_process(pid: u32) -> Result<(), DaemonError> {
    let status = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(DaemonError::ProcessControl {
            message: format!("failed to send TERM to pid {pid}"),
        });
    }

    for _ in 0..30 {
        if !is_process_running(pid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(DaemonError::ProcessControl {
        message: format!("pid {pid} did not exit after TERM"),
    })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn daemon_records_and_clears_current_process() {
        let home = unique_home("record-clear");
        let manager = DaemonManager::new(Some(&home)).expect("manager");
        let spec = manager
            .prepare_run(DaemonRunRequest {
                host: Some("127.0.0.1".to_string()),
                port: Some(8799),
            })
            .expect("prepare");

        assert_eq!(spec.host, "127.0.0.1");
        assert_eq!(spec.port, 8799);
        assert!(!spec.inspection.running);

        let pid = std::process::id();
        let inspection = manager
            .record_process_start(pid, spec.host, spec.port)
            .expect("record");

        assert!(inspection.running);
        assert_eq!(
            inspection.process.as_ref().map(|process| process.pid),
            Some(pid)
        );
        assert!(inspection.process_path.exists());
        assert!(inspection.pid_path.exists());

        manager
            .clear_process_if_matches(Some(pid))
            .expect("clear process");

        let inspection = manager.status().expect("status");
        assert!(!inspection.running);
        assert!(inspection.process.is_none());
        assert!(!inspection.process_path.exists());
        assert!(!inspection.pid_path.exists());
    }

    #[test]
    fn daemon_rejects_empty_host() {
        let home = unique_home("empty-host");
        let manager = DaemonManager::new(Some(&home)).expect("manager");
        let error = manager
            .prepare_run(DaemonRunRequest {
                host: Some(" ".to_string()),
                port: None,
            })
            .expect_err("empty host should fail");

        assert!(matches!(error, DaemonError::EmptyHost));
    }

    #[test]
    fn readonly_manager_does_not_create_missing_runtime_home() {
        let home = unique_home("readonly-missing-home");
        let manager = DaemonManager::open_readonly(Some(&home)).expect("manager");

        let inspection = manager.status_observational().expect("status");

        assert!(!home.exists());
        assert_eq!(warning_codes(&inspection), vec!["runtime_home_missing"]);
    }

    #[test]
    fn status_warns_when_runtime_dir_is_missing_under_existing_home() {
        let home = unique_home("missing-runtime-dir");
        std::fs::create_dir_all(&home).expect("home");
        let manager = DaemonManager::open_readonly(Some(&home)).expect("manager");

        let inspection = manager.status_observational().expect("status");

        assert!(home.exists());
        assert!(!home.join("runtime").exists());
        assert_eq!(warning_codes(&inspection), vec!["runtime_dir_missing"]);
    }

    #[test]
    fn status_warns_when_process_metadata_is_missing_under_runtime_dir() {
        let home = unique_home("missing-process-path");
        std::fs::create_dir_all(home.join("runtime")).expect("runtime");
        let manager = DaemonManager::open_readonly(Some(&home)).expect("manager");

        let inspection = manager.status_observational().expect("status");

        assert_eq!(warning_codes(&inspection), vec!["process_path_missing"]);
    }

    #[test]
    fn status_warns_when_pid_file_exists_without_process_metadata() {
        let home = unique_home("pid-without-metadata");
        let runtime_dir = home.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime");
        std::fs::write(runtime_dir.join("tentgent.pid"), "12345\n").expect("pid");
        let manager = DaemonManager::open_readonly(Some(&home)).expect("manager");

        let inspection = manager.status_observational().expect("status");

        assert_eq!(warning_codes(&inspection), vec!["pid_path_stale"]);
        assert!(runtime_dir.join("tentgent.pid").exists());
    }

    #[test]
    fn observational_status_preserves_dead_process_metadata() {
        let home = unique_home("dead-observational");
        write_dead_process_metadata(&home);
        let manager = DaemonManager::open_readonly(Some(&home)).expect("manager");

        let inspection = manager.status_observational().expect("status");

        assert!(!inspection.running);
        assert!(inspection.process.is_some());
        assert_eq!(warning_codes(&inspection), vec!["process_metadata_stale"]);
        assert!(inspection.process_path.exists());
        assert!(inspection.pid_path.exists());
    }

    #[test]
    fn cleanup_status_removes_dead_process_metadata() {
        let home = unique_home("dead-cleanup");
        write_dead_process_metadata(&home);
        let manager = DaemonManager::open_readonly(Some(&home)).expect("manager");

        let inspection = manager.status_with_cleanup().expect("status");

        assert!(!inspection.running);
        assert!(inspection.process.is_none());
        assert_eq!(warning_codes(&inspection), vec!["process_metadata_stale"]);
        assert!(!inspection.process_path.exists());
        assert!(!inspection.pid_path.exists());
    }

    fn write_dead_process_metadata(home: &Path) {
        let runtime_dir = home.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime");
        let metadata = DaemonProcessMetadata {
            pid: 999_999_999,
            host: "127.0.0.1".to_string(),
            port: 8790,
            started_at: created_at_now().expect("created_at"),
        };
        write_process_metadata(&runtime_dir.join("daemon.toml"), &metadata).expect("metadata");
        write_pid_file(&runtime_dir.join("tentgent.pid"), metadata.pid).expect("pid");
    }

    fn warning_codes(inspection: &DaemonInspection) -> Vec<&str> {
        inspection
            .warnings
            .iter()
            .map(|warning| warning.code.as_str())
            .collect()
    }

    fn unique_home(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("tentgent-daemon-{label}-{nanos}"))
    }
}
