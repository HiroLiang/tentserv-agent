use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("failed to resolve the Tentgent runtime-home from platform directories")]
    ProjectDirsUnavailable,
    #[error("session reference `{0}` was not found")]
    NotFound(String),
    #[error("session reference `{0}` is ambiguous; multiple stored sessions share that prefix")]
    AmbiguousRef(String),
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
}
