use std::{
    env, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::error::DaemonError;

const HOME_ENV: &str = "TENTGENT_HOME";
const RUNTIME_ENV: &str = "TENTGENT_RUNTIME_DIR";
const LOG_ENV: &str = "TENTGENT_LOG_DIR";

pub const DEFAULT_DAEMON_HOST: &str = "127.0.0.1";
pub const DEFAULT_DAEMON_PORT: u16 = 8790;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonProcessMetadata {
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub started_at: String,
}

#[derive(Debug, Clone)]
pub struct DaemonStorePaths {
    pub home_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub log_dir: PathBuf,
    pub process_path: PathBuf,
    pub pid_path: PathBuf,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
}

impl DaemonStorePaths {
    pub fn resolve(home_override: Option<&Path>) -> Result<Self, DaemonError> {
        let home_dir = home_override
            .map(Path::to_path_buf)
            .or_else(|| read_env_path(HOME_ENV))
            .unwrap_or(default_home_dir()?);
        let runtime_dir = read_env_path(RUNTIME_ENV).unwrap_or_else(|| home_dir.join("runtime"));
        let log_dir = read_env_path(LOG_ENV).unwrap_or_else(|| home_dir.join("logs"));

        Ok(Self {
            process_path: runtime_dir.join("daemon.toml"),
            pid_path: runtime_dir.join("tentgent.pid"),
            stdout_log_path: log_dir.join("daemon.stdout.log"),
            stderr_log_path: log_dir.join("daemon.stderr.log"),
            home_dir,
            runtime_dir,
            log_dir,
        })
    }

    pub fn ensure_layout(&self) -> Result<(), DaemonError> {
        fs::create_dir_all(&self.runtime_dir)?;
        fs::create_dir_all(&self.log_dir)?;
        Ok(())
    }
}

pub fn created_at_now() -> Result<String, DaemonError> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

pub fn write_process_metadata(
    path: &Path,
    metadata: &DaemonProcessMetadata,
) -> Result<(), DaemonError> {
    let body = toml::to_string_pretty(metadata)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn read_process_metadata(path: &Path) -> Result<DaemonProcessMetadata, DaemonError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| DaemonError::ProcessParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

pub fn write_pid_file(path: &Path, pid: u32) -> Result<(), DaemonError> {
    fs::write(path, format!("{pid}\n"))?;
    Ok(())
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    let value = env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn default_home_dir() -> Result<PathBuf, DaemonError> {
    let project_dirs = ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(DaemonError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}
