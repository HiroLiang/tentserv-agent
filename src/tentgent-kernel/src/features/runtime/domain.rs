//! Runtime initialization, bootstrap, and Python asset domain types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const PYTHON_PROJECT_ENV: &str = "TENTGENT_PYTHON_DIR";
pub const PYTHON_ENV_DIR_ENV: &str = "TENTGENT_PYTHON_ENV_DIR";
pub const DEFAULT_RUNTIME_IDLE_SECONDS: u64 = 300;
pub const DEFAULT_MODEL_IDLE_SECONDS: u64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRuntimeIdlePolicy {
    pub runtime_idle_seconds: u64,
    pub model_idle_seconds: u64,
}

impl ModelRuntimeIdlePolicy {
    pub fn new(runtime_idle_seconds: u64, model_idle_seconds: u64) -> Result<Self, String> {
        if model_idle_seconds > runtime_idle_seconds {
            return Err(format!(
                "model_idle_seconds ({model_idle_seconds}) must be less than or equal to runtime_idle_seconds ({runtime_idle_seconds})"
            ));
        }
        Ok(Self {
            runtime_idle_seconds,
            model_idle_seconds,
        })
    }

    pub fn from_overrides(
        runtime_idle_seconds: Option<u64>,
        model_idle_seconds: Option<u64>,
    ) -> Result<Self, String> {
        Self::new(
            runtime_idle_seconds.unwrap_or(DEFAULT_RUNTIME_IDLE_SECONDS),
            model_idle_seconds.unwrap_or(DEFAULT_MODEL_IDLE_SECONDS),
        )
    }
}

impl Default for ModelRuntimeIdlePolicy {
    fn default() -> Self {
        Self {
            runtime_idle_seconds: DEFAULT_RUNTIME_IDLE_SECONDS,
            model_idle_seconds: DEFAULT_MODEL_IDLE_SECONDS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BootstrapProfile {
    #[default]
    Base,
    LocalModel,
    Training,
    Full,
}

impl BootstrapProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::LocalModel => "local-model",
            Self::Training => "training",
            Self::Full => "full",
        }
    }
}

impl std::fmt::Display for BootstrapProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapRuntimeInput {
    pub project_dir: Option<PathBuf>,
    pub python_env_dir: Option<PathBuf>,
    pub uv_path: Option<PathBuf>,
    pub profile: BootstrapProfile,
    pub dry_run: bool,
    pub print_plan: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBootstrapPlan {
    pub project_dir: PathBuf,
    pub python_env_dir: PathBuf,
    pub script_path: PathBuf,
    pub uv_path: Option<PathBuf>,
    pub profile: BootstrapProfile,
    pub dry_run: bool,
    pub print_plan: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBootstrapOutcome {
    pub status: RuntimeBootstrapStatus,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeBootstrapStatus {
    Succeeded,
    Failed,
}

impl RuntimeBootstrapStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PythonRuntimeResolutionInput {
    pub project_dir: Option<PathBuf>,
    pub python_env_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonRuntimeLayout {
    pub project_dir: PathBuf,
    pub env_dir: PathBuf,
    pub source: PythonRuntimeSource,
}

impl PythonRuntimeLayout {
    pub fn pyproject_path(&self) -> PathBuf {
        self.project_dir.join("pyproject.toml")
    }

    pub fn python_src_dir(&self) -> PathBuf {
        self.project_dir.join("src")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PythonRuntimeSource {
    EnvironmentOverride,
    InstalledPrefix,
    DevelopmentSource,
}

impl PythonRuntimeSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EnvironmentOverride => "environment",
            Self::InstalledPrefix => "installed-prefix",
            Self::DevelopmentSource => "development-source",
        }
    }
}

impl std::fmt::Display for PythonRuntimeSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeEntrypoint {
    HfSnapshot,
    ModelRuntimeDaemon,
}

impl RuntimeEntrypoint {
    pub const fn script_name(self) -> &'static str {
        match self {
            Self::HfSnapshot => "tentgent-hf-snapshot",
            Self::ModelRuntimeDaemon => "tentgent-model-runtime-daemon",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInitState {
    pub home_dir: PathBuf,
    pub python_env_dir: PathBuf,
    pub bootstrap_dir: PathBuf,
    pub uv_cache_dir: PathBuf,
    pub python: PythonRuntimeState,
    pub profiles: Vec<RuntimeProfileState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonRuntimeState {
    pub env_exists: bool,
    pub binary_path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProfileState {
    pub profile: BootstrapProfile,
    pub readiness: RuntimeReadiness,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeReadiness {
    Ready,
    Missing,
    Stale,
    Unsupported,
    Unknown,
}
