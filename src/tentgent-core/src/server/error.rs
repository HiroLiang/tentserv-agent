use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("failed to resolve the Tentgent runtime-home from platform directories")]
    ProjectDirsUnavailable,
    #[error("server host must not be empty")]
    EmptyHost,
    #[error("server reference `{0}` was not found")]
    NotFound(String),
    #[error("server reference `{0}` is ambiguous; multiple stored servers share that prefix")]
    AmbiguousRef(String),
    #[error("server `{0}` is already running")]
    AlreadyRunning(String),
    #[error("server `{0}` is not running")]
    NotRunning(String),
    #[error("failed to parse server spec `{path}`: {message}")]
    SpecParse { path: PathBuf, message: String },
    #[error("failed to parse server process metadata `{path}`: {message}")]
    ProcessParse { path: PathBuf, message: String },
    #[error("failed to control server process: {message}")]
    ProcessControl { message: String },
    #[error(transparent)]
    Model(#[from] crate::model::ModelError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
}
