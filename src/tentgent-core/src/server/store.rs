use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::error::ServerError;

const HOME_ENV: &str = "TENTGENT_HOME";

pub const DEFAULT_SERVER_HOST: &str = "127.0.0.1";
pub const DEFAULT_SERVER_PORT: u16 = 8000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSpec {
    pub server_ref: String,
    pub short_ref: String,
    pub model_ref: String,
    pub host: String,
    pub port: u16,
    pub lazy_load: bool,
    pub idle_seconds: Option<u64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchMode {
    #[serde(rename = "foreground")]
    Foreground,
    #[serde(rename = "background")]
    Background,
}

impl LaunchMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Background => "background",
        }
    }
}

impl fmt::Display for LaunchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerProcessMetadata {
    pub pid: u32,
    pub launch_mode: LaunchMode,
    pub started_at: String,
}

#[derive(Debug, Clone)]
pub struct ServerStorePaths {
    pub home_dir: PathBuf,
    pub servers_dir: PathBuf,
}

impl ServerStorePaths {
    pub fn resolve(home_override: Option<&Path>) -> Result<Self, ServerError> {
        let home_dir = home_override
            .map(Path::to_path_buf)
            .or_else(|| read_env_path(HOME_ENV))
            .unwrap_or(default_home_dir()?);

        Ok(Self {
            servers_dir: home_dir.join("servers"),
            home_dir,
        })
    }

    pub fn ensure_layout(&self) -> Result<(), ServerError> {
        fs::create_dir_all(&self.servers_dir)?;
        Ok(())
    }

    pub fn server_dir(&self, server_ref: &str) -> PathBuf {
        self.servers_dir.join(server_ref)
    }

    pub fn server_toml_path(&self, server_ref: &str) -> PathBuf {
        self.server_dir(server_ref).join("server.toml")
    }

    pub fn process_toml_path(&self, server_ref: &str) -> PathBuf {
        self.server_dir(server_ref).join("process.toml")
    }

    pub fn stdout_log_path(&self, server_ref: &str) -> PathBuf {
        self.server_dir(server_ref).join("stdout.log")
    }

    pub fn stderr_log_path(&self, server_ref: &str) -> PathBuf {
        self.server_dir(server_ref).join("stderr.log")
    }
}

pub fn created_at_now() -> Result<String, ServerError> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

pub fn write_server_spec(path: &Path, spec: &ServerSpec) -> Result<(), ServerError> {
    let body = toml::to_string_pretty(spec)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn read_server_spec(path: &Path) -> Result<ServerSpec, ServerError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| ServerError::SpecParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

pub fn write_process_metadata(
    path: &Path,
    metadata: &ServerProcessMetadata,
) -> Result<(), ServerError> {
    let body = toml::to_string_pretty(metadata)?;
    fs::write(path, body)?;
    Ok(())
}

pub fn read_process_metadata(path: &Path) -> Result<ServerProcessMetadata, ServerError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| ServerError::ProcessParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
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

fn default_home_dir() -> Result<PathBuf, ServerError> {
    let project_dirs = ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(ServerError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}
