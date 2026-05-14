//! Capability probe interface and probe inputs.

use std::path::{Path, PathBuf};

use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::domain::RuntimeLayout;
use crate::foundation::platform::domain::PlatformFacts;
use crate::foundation::platform::domain::{Architecture, GpuFacts, OperatingSystem};

use super::domain::{
    BackendCapability, BackendKind, CapabilityProbeReport, CapabilityState, RuntimeCapabilityState,
    RuntimeProfileCapability,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityProbeInput {
    pub platform: PlatformFacts,
    pub layout: RuntimeLayout,
    pub include_heavy_checks: bool,
}

pub trait CapabilityProbe {
    fn probe(&self, input: CapabilityProbeInput) -> KernelResult<CapabilityProbeReport>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StdCapabilityProbe;

impl CapabilityProbe for StdCapabilityProbe {
    fn probe(&self, input: CapabilityProbeInput) -> KernelResult<CapabilityProbeReport> {
        let runtime = runtime_capability_state(&input.layout, input.include_heavy_checks);
        let backends = backend_capabilities(
            &input.platform.os,
            &input.platform.arch,
            &input.platform.gpu,
            input.include_heavy_checks,
        );

        Ok(CapabilityProbeReport {
            platform: input.platform,
            runtime,
            backends,
        })
    }
}

fn runtime_capability_state(
    layout: &RuntimeLayout,
    include_heavy_checks: bool,
) -> RuntimeCapabilityState {
    RuntimeCapabilityState {
        home_dir: layout.home_dir.clone(),
        python_env_dir: layout.python_env_dir.clone(),
        profiles: vec![
            base_profile_capability(layout),
            unchecked_profile_capability(BootstrapProfile::LocalModel, include_heavy_checks),
            unchecked_profile_capability(BootstrapProfile::Training, include_heavy_checks),
            unchecked_profile_capability(BootstrapProfile::Full, include_heavy_checks),
        ],
    }
}

fn base_profile_capability(layout: &RuntimeLayout) -> RuntimeProfileCapability {
    let python = python_binary_path(&layout.python_env_dir);
    let ready = python.exists();

    RuntimeProfileCapability {
        profile: BootstrapProfile::Base,
        readiness: if ready {
            RuntimeReadiness::Ready
        } else {
            RuntimeReadiness::Missing
        },
        message: Some(if ready {
            "base runtime profile is installed".to_string()
        } else {
            "base runtime profile is missing".to_string()
        }),
        next_step: (!ready).then(|| "run `tentgent runtime bootstrap --profile base`".to_string()),
    }
}

fn unchecked_profile_capability(
    profile: BootstrapProfile,
    include_heavy_checks: bool,
) -> RuntimeProfileCapability {
    RuntimeProfileCapability {
        profile,
        readiness: RuntimeReadiness::Unknown,
        message: Some(if include_heavy_checks {
            "profile-specific dependency checks are not implemented yet".to_string()
        } else {
            "profile-specific dependency checks were skipped".to_string()
        }),
        next_step: Some(format!(
            "run `tentgent runtime bootstrap --profile {}` before using this profile",
            profile.name()
        )),
    }
}

fn backend_capabilities(
    os: &OperatingSystem,
    arch: &Architecture,
    gpu: &GpuFacts,
    include_heavy_checks: bool,
) -> Vec<BackendCapability> {
    vec![
        local_model_backend(BackendKind::CpuGguf, include_heavy_checks),
        local_model_backend(BackendKind::SafetensorsPeft, include_heavy_checks),
        mlx_backend(os, arch, gpu, include_heavy_checks),
        profile_backend(
            BackendKind::Training,
            "training profile readiness is required",
            "run `tentgent runtime bootstrap --profile training`",
            include_heavy_checks,
        ),
        profile_backend(
            BackendKind::Embedding,
            "embedding backend readiness is not implemented yet",
            "install the future embedding runtime profile before using local embeddings",
            include_heavy_checks,
        ),
        profile_backend(
            BackendKind::Rerank,
            "rerank backend readiness is not implemented yet",
            "install the future rerank runtime profile before using local rerank",
            include_heavy_checks,
        ),
    ]
}

fn local_model_backend(backend: BackendKind, include_heavy_checks: bool) -> BackendCapability {
    profile_backend(
        backend,
        "local-model profile readiness is required",
        "run `tentgent runtime bootstrap --profile local-model`",
        include_heavy_checks,
    )
}

fn mlx_backend(
    os: &OperatingSystem,
    arch: &Architecture,
    gpu: &GpuFacts,
    include_heavy_checks: bool,
) -> BackendCapability {
    if !matches!(os, OperatingSystem::Macos) || !matches!(arch, Architecture::Aarch64) {
        return BackendCapability {
            backend: BackendKind::Mlx,
            state: CapabilityState::Unsupported,
            message: Some("MLX is supported only on Apple Silicon macOS".to_string()),
            next_step: None,
        };
    }

    if gpu
        .metal
        .as_ref()
        .map(|metal| metal.visible)
        .unwrap_or(false)
    {
        return profile_backend(
            BackendKind::Mlx,
            "Metal is visible; MLX Python dependencies still need profile checks",
            "run `tentgent runtime bootstrap --profile local-model`",
            include_heavy_checks,
        );
    }

    BackendCapability {
        backend: BackendKind::Mlx,
        state: CapabilityState::Blocked,
        message: Some("Metal is not visible on this Apple Silicon macOS host".to_string()),
        next_step: None,
    }
}

fn profile_backend(
    backend: BackendKind,
    message: &str,
    next_step: &str,
    include_heavy_checks: bool,
) -> BackendCapability {
    BackendCapability {
        backend,
        state: CapabilityState::Unknown,
        message: Some(if include_heavy_checks {
            format!("{message}; heavy backend checks are not implemented yet")
        } else {
            format!("{message}; heavy backend checks were skipped")
        }),
        next_step: Some(next_step.to_string()),
    }
}

fn python_binary_path(env_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        env_dir.join("Scripts/python.exe")
    } else {
        env_dir.join("bin/python")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::foundation::layout::domain::RuntimeLayout;
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, MetalFacts, OperatingSystem, PlatformFacts,
    };

    use super::*;

    #[test]
    fn probe_returns_report_not_manifest() {
        let probe = StdCapabilityProbe;

        let report = probe
            .probe(CapabilityProbeInput {
                platform: platform_facts(),
                layout: runtime_layout("/tmp/tentgent-capability-probe"),
                include_heavy_checks: false,
            })
            .expect("probe capabilities");

        assert_eq!(report.platform.os, OperatingSystem::Macos);
        assert_eq!(report.runtime.profiles.len(), 4);
        assert!(report
            .backends
            .iter()
            .any(|backend| backend.backend == BackendKind::Mlx));
    }

    fn platform_facts() -> PlatformFacts {
        PlatformFacts {
            os: OperatingSystem::Macos,
            arch: Architecture::Aarch64,
            libc: None,
            cpu: CpuFacts {
                vendor: Some("Apple".to_string()),
                brand: Some("Apple M3 Pro".to_string()),
                features: vec!["neon".to_string()],
            },
            gpu: GpuFacts {
                devices: Vec::new(),
                cuda: None,
                metal: Some(MetalFacts { visible: true }),
            },
        }
    }

    fn runtime_layout(root: &str) -> RuntimeLayout {
        let home_dir = PathBuf::from(root);
        RuntimeLayout {
            config_path: home_dir.join("config.toml"),
            models_dir: home_dir.join("models"),
            adapters_dir: home_dir.join("adapters"),
            datasets_dir: home_dir.join("datasets"),
            sessions_dir: home_dir.join("sessions"),
            servers_dir: home_dir.join("servers"),
            train_dir: home_dir.join("train"),
            cache_dir: home_dir.join("cache"),
            runtime_dir: home_dir.join("runtime"),
            logs_dir: home_dir.join("logs"),
            locks_dir: home_dir.join("locks"),
            python_env_dir: home_dir.join("runtime/python-env"),
            bootstrap_dir: home_dir.join("runtime/bootstrap"),
            bootstrap_uv_dir: home_dir.join("runtime/bootstrap/uv"),
            bootstrap_uv_cache_dir: home_dir.join("runtime/bootstrap/uv-cache"),
            capability_manifest_path: home_dir.join("runtime/capabilities.toml"),
            home_dir,
        }
    }
}
