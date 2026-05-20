//! Capability readiness domain types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
use crate::foundation::platform::PlatformFacts;

pub const CAPABILITY_SCHEMA_VERSION: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineCapabilities {
    pub schema_version: u32,
    pub generated_at: Option<String>,
    pub platform: PlatformFacts,
    pub runtime: RuntimeCapabilityState,
    pub backends: Vec<BackendCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCapabilityState {
    pub home_dir: PathBuf,
    pub python_env_dir: PathBuf,
    pub profiles: Vec<RuntimeProfileCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProfileCapability {
    pub profile: BootstrapProfile,
    pub readiness: RuntimeReadiness,
    pub message: Option<String>,
    pub next_step: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCapability {
    pub backend: BackendKind,
    pub state: CapabilityState,
    pub message: Option<String>,
    pub next_step: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendKind {
    CpuGguf,
    SafetensorsPeft,
    Mlx,
    MlxVlm,
    MlxAudio,
    MlxDiffusion,
    Training,
    Embedding,
    Rerank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityState {
    Ready,
    Missing,
    Blocked,
    Unsupported,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityCheck {
    pub state: CapabilityState,
    pub message: Option<String>,
    pub next_step: Option<String>,
}
