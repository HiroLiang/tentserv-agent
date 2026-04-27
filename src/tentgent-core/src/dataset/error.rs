use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DatasetError {
    #[error("path does not exist: {0}")]
    MissingPath(PathBuf),
    #[error("path is not a supported dataset file or directory: {0}")]
    UnsupportedPath(PathBuf),
    #[error("export destination already exists and is not an empty directory: {0}")]
    ExportDestinationNotEmpty(PathBuf),
    #[error("export destination exists but is not a directory: {0}")]
    ExportDestinationNotDirectory(PathBuf),
    #[error("failed to walk `{path}`: {message}")]
    Walk { path: PathBuf, message: String },
    #[error("failed to resolve the Tentgent runtime-home from platform directories")]
    ProjectDirsUnavailable,
    #[error("unsupported dataset layout: {reason}")]
    UnsupportedLayout { reason: String },
    #[error("dataset reference `{0}` was not found")]
    NotFound(String),
    #[error("dataset reference `{0}` is ambiguous; multiple stored datasets share that prefix")]
    AmbiguousRef(String),
    #[error("failed to parse metadata file `{path}`: {message}")]
    MetadataParse { path: PathBuf, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
}
