use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("path does not exist: {0}")]
    MissingPath(PathBuf),
    #[error("path is not a supported adapter directory: {0}")]
    UnsupportedPath(PathBuf),
    #[error("failed to walk `{path}`: {message}")]
    Walk { path: PathBuf, message: String },
    #[error("failed to resolve the Tentgent runtime-home from platform directories")]
    ProjectDirsUnavailable,
    #[error("unsupported adapter layout: {reason}")]
    UnsupportedLayout { reason: String },
    #[error("adapter reference `{0}` was not found")]
    NotFound(String),
    #[error("adapter reference `{0}` is ambiguous; multiple stored adapters share that prefix")]
    AmbiguousRef(String),
    #[error("adapter `{adapter_ref}` is still referenced by server spec(s): {server_refs}")]
    InUse {
        adapter_ref: String,
        server_refs: String,
    },
    #[error("adapter base model `{adapter_base}` does not match local model `{model_base}`")]
    BaseModelMismatch {
        adapter_base: String,
        model_base: String,
    },
    #[error("adapter base revision `{adapter_revision}` does not match local model revision `{model_revision}`")]
    BaseRevisionMismatch {
        adapter_revision: String,
        model_revision: String,
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
