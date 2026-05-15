//! Standard machine capability probe implementation.

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::capabilities::domain::{
    BackendCapability, BackendKind, CapabilityState, MachineCapabilities, RuntimeCapabilityState,
    RuntimeProfileCapability,
};
use crate::capabilities::ports::MachineCapabilitiesProbe;
use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{Architecture, OperatingSystem, PlatformFacts};

const CAPABILITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default)]
pub struct StdMachineCapabilitiesProbe;

impl MachineCapabilitiesProbe for StdMachineCapabilitiesProbe {
    fn probe(
        &self,
        layout: &RuntimeLayout,
        platform: &PlatformFacts,
    ) -> KernelResult<MachineCapabilities> {
        Ok(MachineCapabilities {
            schema_version: CAPABILITY_SCHEMA_VERSION,
            generated_at: generated_at(),
            platform: platform.clone(),
            runtime: RuntimeCapabilityState {
                home_dir: layout.home_dir.clone(),
                python_env_dir: layout.python_env_dir.clone(),
                profiles: runtime_profiles(layout),
            },
            backends: backend_capabilities(layout, platform),
        })
    }
}

fn runtime_profiles(layout: &RuntimeLayout) -> Vec<RuntimeProfileCapability> {
    [
        BootstrapProfile::Base,
        BootstrapProfile::LocalModel,
        BootstrapProfile::Training,
        BootstrapProfile::Full,
    ]
    .into_iter()
    .map(|profile| runtime_profile(layout, profile))
    .collect()
}

fn runtime_profile(layout: &RuntimeLayout, profile: BootstrapProfile) -> RuntimeProfileCapability {
    if !layout.python_env_dir.is_dir() {
        return RuntimeProfileCapability {
            profile,
            readiness: RuntimeReadiness::Missing,
            message: Some("managed Python environment is missing".to_string()),
            next_step: Some("bootstrap the requested runtime profile".to_string()),
        };
    }

    match profile {
        BootstrapProfile::Base => RuntimeProfileCapability {
            profile,
            readiness: RuntimeReadiness::Ready,
            message: Some("managed Python environment exists".to_string()),
            next_step: None,
        },
        BootstrapProfile::LocalModel | BootstrapProfile::Training | BootstrapProfile::Full => {
            RuntimeProfileCapability {
                profile,
                readiness: RuntimeReadiness::Unknown,
                message: Some("profile-specific Python packages have not been probed".to_string()),
                next_step: Some(
                    "run dependency probes after bootstrapping the profile".to_string(),
                ),
            }
        }
    }
}

fn backend_capabilities(
    layout: &RuntimeLayout,
    platform: &PlatformFacts,
) -> Vec<BackendCapability> {
    [
        BackendKind::CpuGguf,
        BackendKind::SafetensorsPeft,
        BackendKind::Mlx,
        BackendKind::Training,
        BackendKind::Embedding,
        BackendKind::Rerank,
    ]
    .into_iter()
    .map(|backend| backend_capability(layout, platform, backend))
    .collect()
}

fn backend_capability(
    layout: &RuntimeLayout,
    platform: &PlatformFacts,
    backend: BackendKind,
) -> BackendCapability {
    match backend {
        BackendKind::Mlx if !is_macos_apple_silicon(platform) => BackendCapability {
            backend,
            state: CapabilityState::Unsupported,
            message: Some("MLX requires Apple Silicon macOS".to_string()),
            next_step: None,
        },
        BackendKind::Training if !is_known_runtime_os(platform) => BackendCapability {
            backend,
            state: CapabilityState::Unsupported,
            message: Some("training is unsupported on this operating system".to_string()),
            next_step: None,
        },
        BackendKind::Embedding | BackendKind::Rerank => BackendCapability {
            backend,
            state: CapabilityState::Unknown,
            message: Some("backend capability is not part of the current probe set".to_string()),
            next_step: Some(
                "refresh capability state after backend support is implemented".to_string(),
            ),
        },
        _ if !layout.python_env_dir.is_dir() => BackendCapability {
            backend,
            state: CapabilityState::Missing,
            message: Some("managed Python environment is missing".to_string()),
            next_step: Some("bootstrap the runtime before using local backends".to_string()),
        },
        _ => BackendCapability {
            backend,
            state: CapabilityState::Unknown,
            message: Some("backend dependencies have not been probed".to_string()),
            next_step: Some("run backend dependency probes".to_string()),
        },
    }
}

fn is_macos_apple_silicon(platform: &PlatformFacts) -> bool {
    platform.os == OperatingSystem::Macos && platform.arch == Architecture::Aarch64
}

fn is_known_runtime_os(platform: &PlatformFacts) -> bool {
    matches!(
        platform.os,
        OperatingSystem::Macos | OperatingSystem::Linux | OperatingSystem::Windows
    )
}

fn generated_at() -> Option<String> {
    OffsetDateTime::now_utc().format(&Rfc3339).ok()
}
