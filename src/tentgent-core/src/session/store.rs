use std::{
    env, fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use super::error::SessionError;

const HOME_ENV: &str = "TENTGENT_HOME";

pub const SESSION_SCHEMA: &str = "tentgent.session.v1";
pub const SESSION_MESSAGE_SCHEMA: &str = "tentgent.session.message.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub schema: String,
    pub session_ref: String,
    pub short_ref: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub default_server_ref: Option<String>,
    pub adapter_ref: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct SessionStorePaths {
    pub home_dir: PathBuf,
    pub sessions_dir: PathBuf,
}

impl SessionStorePaths {
    pub fn resolve(home_override: Option<&Path>) -> Result<Self, SessionError> {
        let home_dir = home_override
            .map(Path::to_path_buf)
            .or_else(|| read_env_path(HOME_ENV))
            .unwrap_or(default_home_dir()?);
        Ok(Self {
            sessions_dir: home_dir.join("sessions"),
            home_dir,
        })
    }

    pub fn session_dir(&self, session_ref: &str) -> PathBuf {
        self.sessions_dir.join(session_ref)
    }

    pub fn metadata_path(&self, session_ref: &str) -> PathBuf {
        self.session_dir(session_ref).join("session.toml")
    }

    pub fn messages_path(&self, session_ref: &str) -> PathBuf {
        self.session_dir(session_ref).join("messages.jsonl")
    }
}

pub fn read_session_metadata(path: &Path) -> Result<SessionMetadata, SessionError> {
    let body = fs::read_to_string(path)?;
    toml::from_str(&body).map_err(|err| SessionError::MetadataParse {
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

fn default_home_dir() -> Result<PathBuf, SessionError> {
    let project_dirs = ProjectDirs::from("com", "tentserv", "tentgent")
        .ok_or(SessionError::ProjectDirsUnavailable)?;
    Ok(project_dirs.data_local_dir().to_path_buf())
}
