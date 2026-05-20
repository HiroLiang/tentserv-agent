//! Standard machine capability probe implementation.

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::capabilities::domain::{
    BackendCapability, BackendKind, CapabilityState, MachineCapabilities, RuntimeCapabilityState,
    RuntimeProfileCapability, CAPABILITY_SCHEMA_VERSION,
};
use crate::capabilities::ports::MachineCapabilitiesProbe;
use crate::features::runtime::domain::{BootstrapProfile, PythonRuntimeLayout, RuntimeReadiness};
use crate::features::runtime::infra::{
    probe_python_modules, python_binary_for_env, runtime_profile_modules, training_modules,
    PythonModuleProbe,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::{Architecture, OperatingSystem, PlatformFacts};

#[derive(Debug, Clone, Copy, Default)]
pub struct StdMachineCapabilitiesProbe;

impl MachineCapabilitiesProbe for StdMachineCapabilitiesProbe {
    fn probe(
        &self,
        layout: &RuntimeLayout,
        runtime: Option<&PythonRuntimeLayout>,
        platform: &PlatformFacts,
    ) -> KernelResult<MachineCapabilities> {
        let python_env_dir = runtime
            .map(|runtime| runtime.env_dir.clone())
            .unwrap_or_else(|| layout.python_env_dir.clone());

        Ok(MachineCapabilities {
            schema_version: CAPABILITY_SCHEMA_VERSION,
            generated_at: generated_at(),
            platform: platform.clone(),
            runtime: RuntimeCapabilityState {
                home_dir: layout.home_dir.clone(),
                python_env_dir: python_env_dir.clone(),
                profiles: runtime_profiles(&python_env_dir),
            },
            backends: backend_capabilities(&python_env_dir, platform),
        })
    }
}

fn runtime_profiles(python_env_dir: &std::path::Path) -> Vec<RuntimeProfileCapability> {
    [
        BootstrapProfile::Base,
        BootstrapProfile::LocalModel,
        BootstrapProfile::Training,
        BootstrapProfile::Full,
    ]
    .into_iter()
    .map(|profile| runtime_profile(python_env_dir, profile))
    .collect()
}

fn runtime_profile(
    python_env_dir: &std::path::Path,
    profile: BootstrapProfile,
) -> RuntimeProfileCapability {
    if !python_env_dir.is_dir() {
        return RuntimeProfileCapability {
            profile,
            readiness: RuntimeReadiness::Missing,
            message: Some("managed Python environment is missing".to_string()),
            next_step: Some("bootstrap the requested runtime profile".to_string()),
        };
    }

    let python_binary = python_binary_for_env(python_env_dir);
    if !python_binary.is_file() {
        return RuntimeProfileCapability {
            profile,
            readiness: RuntimeReadiness::Missing,
            message: Some("managed Python interpreter is missing".to_string()),
            next_step: Some("bootstrap the requested runtime profile".to_string()),
        };
    }

    let modules = runtime_profile_modules(profile);
    let probe = probe_python_modules(&python_binary, &modules);
    match probe {
        PythonModuleProbe::Ready => RuntimeProfileCapability {
            profile,
            readiness: RuntimeReadiness::Ready,
            message: Some(if modules.is_empty() {
                "managed Python interpreter is available".to_string()
            } else {
                format!("{} profile dependencies are importable", profile.as_str())
            }),
            next_step: None,
        },
        PythonModuleProbe::Missing { modules } => RuntimeProfileCapability {
            profile,
            readiness: RuntimeReadiness::Missing,
            message: Some(format!("missing Python modules: {}", modules.join(", "))),
            next_step: Some(format!(
                "run `tentgent runtime bootstrap --profile {}`",
                profile.as_str()
            )),
        },
        PythonModuleProbe::Failed { detail } => RuntimeProfileCapability {
            profile,
            readiness: RuntimeReadiness::Unknown,
            message: Some(format!("failed to probe Python modules: {detail}")),
            next_step: Some("rerun doctor or inspect the managed Python environment".to_string()),
        },
    }
}

fn backend_capabilities(
    python_env_dir: &std::path::Path,
    platform: &PlatformFacts,
) -> Vec<BackendCapability> {
    [
        BackendKind::CpuGguf,
        BackendKind::SafetensorsPeft,
        BackendKind::Mlx,
        BackendKind::MlxVlm,
        BackendKind::Training,
        BackendKind::Embedding,
        BackendKind::Rerank,
    ]
    .into_iter()
    .map(|backend| backend_capability(python_env_dir, platform, backend))
    .collect()
}

fn backend_capability(
    python_env_dir: &std::path::Path,
    platform: &PlatformFacts,
    backend: BackendKind,
) -> BackendCapability {
    match backend {
        BackendKind::Mlx | BackendKind::MlxVlm if !is_macos_apple_silicon(platform) => {
            BackendCapability {
                backend,
                state: CapabilityState::Unsupported,
                message: Some("MLX requires Apple Silicon macOS".to_string()),
                next_step: None,
            }
        }
        BackendKind::Training if !is_known_runtime_os(platform) => BackendCapability {
            backend,
            state: CapabilityState::Unsupported,
            message: Some("training is unsupported on this operating system".to_string()),
            next_step: None,
        },
        _ if !python_env_dir.is_dir() => BackendCapability {
            backend,
            state: CapabilityState::Missing,
            message: Some("managed Python environment is missing".to_string()),
            next_step: Some("bootstrap the runtime before using local backends".to_string()),
        },
        _ => probed_backend_capability(python_env_dir, backend),
    }
}

fn probed_backend_capability(
    python_env_dir: &std::path::Path,
    backend: BackendKind,
) -> BackendCapability {
    let python_binary = python_binary_for_env(python_env_dir);
    if !python_binary.is_file() {
        return BackendCapability {
            backend,
            state: CapabilityState::Missing,
            message: Some("managed Python interpreter is missing".to_string()),
            next_step: Some("bootstrap the runtime before using local backends".to_string()),
        };
    }

    let modules = backend_modules(backend);
    let probe = probe_python_modules(&python_binary, &modules);
    match probe {
        PythonModuleProbe::Ready => BackendCapability {
            backend,
            state: CapabilityState::Ready,
            message: Some(format!(
                "backend dependencies are importable: {}",
                modules.join(", ")
            )),
            next_step: None,
        },
        PythonModuleProbe::Missing { modules } => BackendCapability {
            backend,
            state: CapabilityState::Missing,
            message: Some(format!("missing Python modules: {}", modules.join(", "))),
            next_step: Some(backend_bootstrap_hint(backend).to_string()),
        },
        PythonModuleProbe::Failed { detail } => BackendCapability {
            backend,
            state: CapabilityState::Unknown,
            message: Some(format!("failed to probe backend dependencies: {detail}")),
            next_step: Some("rerun doctor or inspect the managed Python environment".to_string()),
        },
    }
}

fn backend_modules(backend: BackendKind) -> Vec<&'static str> {
    match backend {
        BackendKind::CpuGguf => vec!["llama_cpp"],
        BackendKind::SafetensorsPeft => vec!["safetensors", "peft", "transformers", "torch"],
        BackendKind::Mlx => vec!["mlx", "mlx_lm"],
        BackendKind::MlxVlm => vec!["mlx", "mlx_vlm"],
        BackendKind::Training => training_modules(),
        BackendKind::Embedding => vec!["safetensors", "peft", "transformers", "torch"],
        BackendKind::Rerank => vec!["safetensors", "transformers", "torch"],
    }
}

fn backend_bootstrap_hint(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::CpuGguf
        | BackendKind::SafetensorsPeft
        | BackendKind::Mlx
        | BackendKind::MlxVlm
        | BackendKind::Embedding
        | BackendKind::Rerank => "run `tentgent runtime bootstrap --profile local-model`",
        BackendKind::Training => "run `tentgent runtime bootstrap --profile training`",
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
