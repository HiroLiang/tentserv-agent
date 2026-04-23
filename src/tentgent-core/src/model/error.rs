use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("path does not exist: {0}")]
    MissingPath(PathBuf),
    #[error("path is not a regular file or directory: {0}")]
    UnsupportedPath(PathBuf),
    #[error("failed to walk `{path}`: {message}")]
    Walk { path: PathBuf, message: String },
    #[error("failed to resolve the Tentgent runtime-home from platform directories")]
    ProjectDirsUnavailable,
    #[error("unsupported model layout: {reason}")]
    UnsupportedLayout { reason: String },
    #[error("model reference `{0}` was not found")]
    NotFound(String),
    #[error("model reference `{0}` is ambiguous; multiple stored models share that prefix")]
    AmbiguousRef(String),
    #[error("model `{model_ref}` is still referenced by server spec(s): {server_refs}")]
    InUse {
        model_ref: String,
        server_refs: String,
    },
    #[error("the Hugging Face snapshot helper is missing at `{path}`")]
    MissingHelper { path: PathBuf },
    #[error("failed to invoke the Hugging Face snapshot helper: {message}")]
    HfHelper { message: String },
    #[error("failed to parse Hugging Face snapshot helper output: {message}")]
    HfHelperOutput { message: String },
    #[error("failed to parse metadata file `{path}`: {message}")]
    MetadataParse { path: PathBuf, message: String },
    #[error(transparent)]
    Auth(#[from] crate::auth::AuthError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
}
