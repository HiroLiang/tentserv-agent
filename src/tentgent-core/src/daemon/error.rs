use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("failed to resolve the Tentgent runtime-home from platform directories")]
    ProjectDirsUnavailable,
    #[error("daemon host must not be empty")]
    EmptyHost,
    #[error("daemon is already running as pid {0}")]
    AlreadyRunning(u32),
    #[error("daemon is not running")]
    NotRunning,
    #[error("failed to parse daemon process metadata `{path}`: {message}")]
    ProcessParse { path: PathBuf, message: String },
    #[error("failed to control daemon process: {message}")]
    ProcessControl { message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TomlDeserialize(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
}
