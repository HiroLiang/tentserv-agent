//! Runtime initialization and bootstrap state.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootstrapProfile {
    Base,
    LocalModel,
    Training,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapRuntimeInput {
    pub profile: BootstrapProfile,
    pub dry_run: bool,
    pub print_plan: bool,
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
