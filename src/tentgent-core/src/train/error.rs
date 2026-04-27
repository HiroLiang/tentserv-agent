#[derive(Debug, thiserror::Error)]
pub enum TrainError {
    #[error(transparent)]
    Adapter(#[from] crate::adapter::AdapterError),
    #[error(transparent)]
    Dataset(#[from] crate::dataset::DatasetError),
    #[error(transparent)]
    Model(#[from] crate::model::ModelError),
    #[error("could not determine the Tentgent project data directory")]
    ProjectDirsUnavailable,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("failed to parse training metadata at {path}: {message}")]
    MetadataParse {
        path: std::path::PathBuf,
        message: String,
    },
    #[error("LoRA train plan reference `{0}` was not found")]
    PlanNotFound(String),
    #[error("LoRA train plan reference `{0}` matched multiple plans")]
    AmbiguousPlanRef(String),
    #[error("LoRA train plan `{plan_ref}` is blocked: {reasons}")]
    PlanBlocked { plan_ref: String, reasons: String },
}
