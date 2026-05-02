use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("failed to resolve the Tentgent runtime-home from platform directories")]
    ProjectDirsUnavailable,
    #[error("invalid session request: {0}")]
    InvalidRequest(String),
    #[error("invalid session reference `{0}`")]
    InvalidReference(String),
    #[error("session reference `{0}` was not found")]
    NotFound(String),
    #[error("session reference `{0}` is ambiguous; multiple stored sessions share that prefix")]
    AmbiguousRef(String),
    #[error("session `{0}` is busy; another writer holds its lock")]
    Busy(String),
    #[error("session context is not supported for chat: {0}")]
    UnsupportedContext(String),
    #[error("session context is too large; selected history must be at most {max_bytes} bytes")]
    ContextTooLarge { max_bytes: usize },
    #[error("server reference `{0}` was not found")]
    ServerNotFound(String),
    #[error("server reference `{0}` is ambiguous; use a longer prefix")]
    ServerAmbiguousRef(String),
    #[error("adapter reference `{0}` was not found")]
    AdapterNotFound(String),
    #[error("adapter reference `{0}` is ambiguous; use a longer prefix")]
    AdapterAmbiguousRef(String),
    #[error("failed to parse session metadata `{path}`: {message}")]
    MetadataParse { path: PathBuf, message: String },
    #[error("failed to parse session messages `{path}` at line {line}: {message}")]
    MessageParse {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("invalid session metadata `{path}`: {message}")]
    InvalidMetadata { path: PathBuf, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
}
