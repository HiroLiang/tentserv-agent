//! Machine-local capability manifests persistence shape.

use serde::{Deserialize, Serialize};

use crate::foundation::platform::PlatformFacts;

use super::domain::{BackendCapability, RuntimeCapabilityState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineCapabilityManifest {
    pub schema: CapabilityManifestSchema,
    pub generated_at: Option<String>,
    pub platform: PlatformFacts,
    pub runtime: RuntimeCapabilityState,
    pub backends: Vec<BackendCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifestSchema {
    pub name: String,
    pub version: u32,
}
