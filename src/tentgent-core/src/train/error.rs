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
    RuntimeAssets(#[from] crate::runtime_assets::RuntimeAssetError),
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
    #[error("LoRA train run reference `{0}` was not found")]
    RunNotFound(String),
    #[error("LoRA train run reference `{0}` matched multiple runs")]
    AmbiguousRunRef(String),
    #[error("another LoRA train run is already running: {0}")]
    RunAlreadyRunning(String),
    #[error("{label} is missing at `{path}`; {hint}")]
    MissingPythonInterpreter {
        label: &'static str,
        path: std::path::PathBuf,
        hint: &'static str,
    },
    #[error("failed to spawn LoRA training runtime: {0}")]
    Spawn(std::io::Error),
    #[error("failed to wait for LoRA training runtime: {0}")]
    Wait(std::io::Error),
    #[error("failed to launch LoRA training worker: {detail}")]
    WorkerLaunch { detail: String },
    #[error("failed to parse LoRA training worker pid: {0}")]
    PidParse(#[from] std::num::ParseIntError),
    #[error("failed to resolve a `tentgent` worker binary; set TENTGENT_CLI_BIN")]
    WorkerBinaryMissing,
    #[error("LoRA training worker exited with status {status}")]
    WorkerExit { status: std::process::ExitStatus },
}
