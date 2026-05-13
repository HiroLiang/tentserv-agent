//! Capability probe interface and probe inputs.

use std::path::{Path, PathBuf};

use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::domain::{LayoutResolveMode, RuntimeLayout};
use crate::foundation::layout::resolver::{RuntimeLayoutResolver, StdRuntimeLayoutResolver};
use crate::foundation::platform::domain::{Architecture, GpuFacts, OperatingSystem};
use crate::foundation::platform::probe::{PlatformProbe, StdPlatformProbe};

use super::domain::{
    BackendCapability, BackendKind, CapabilityProbeReport, CapabilityState, RuntimeCapabilityState,
    RuntimeProfileCapability,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityProbeInput {
    pub include_heavy_checks: bool,
}

pub trait CapabilityProbe {
    fn probe(&self, input: CapabilityProbeInput) -> KernelResult<CapabilityProbeReport>;
}

#[derive(Debug, Clone)]
pub struct StdCapabilityProbe<P = StdPlatformProbe, L = StdRuntimeLayoutResolver> {
    pub platform_probe: P,
    pub layout_resolver: L,
}

impl Default for StdCapabilityProbe {
    fn default() -> Self {
        Self {
            platform_probe: StdPlatformProbe,
            layout_resolver: StdRuntimeLayoutResolver,
        }
    }
}

impl<P, L> StdCapabilityProbe<P, L> {
    pub fn new(platform_probe: P, layout_resolver: L) -> Self {
        Self {
            platform_probe,
            layout_resolver,
        }
    }
}

impl<P, L> CapabilityProbe for StdCapabilityProbe<P, L>
where
    P: PlatformProbe,
    L: RuntimeLayoutResolver,
{
    fn probe(&self, input: CapabilityProbeInput) -> KernelResult<CapabilityProbeReport> {
        let platform = self.platform_probe.query_platform_facts()?;
        let layout = self
            .layout_resolver
            .resolve_runtime_layout(LayoutResolveMode::ReadOnly)?;

        let runtime = runtime_capability_state(&layout, input);
        let backends = backend_capabilities(&platform.os, &platform.arch, &platform.gpu, input);

        Ok(CapabilityProbeReport {
            platform,
            runtime,
            backends,
        })
    }
}

fn runtime_capability_state(
    layout: &RuntimeLayout,
    input: CapabilityProbeInput,
) -> RuntimeCapabilityState {
    RuntimeCapabilityState {
        home_dir: layout.home_dir.clone(),
        python_env_dir: layout.python_env_dir.clone(),
        profiles: vec![
            base_profile_capability(layout),
            unchecked_profile_capability(BootstrapProfile::LocalModel, input),
            unchecked_profile_capability(BootstrapProfile::Training, input),
            unchecked_profile_capability(BootstrapProfile::Full, input),
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
    input: CapabilityProbeInput,
) -> RuntimeProfileCapability {
    RuntimeProfileCapability {
        profile,
        readiness: RuntimeReadiness::Unknown,
        message: Some(if input.include_heavy_checks {
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
    input: CapabilityProbeInput,
) -> Vec<BackendCapability> {
    vec![
        local_model_backend(BackendKind::CpuGguf, input),
        local_model_backend(BackendKind::SafetensorsPeft, input),
        mlx_backend(os, arch, gpu, input),
        profile_backend(
            BackendKind::Training,
            "training profile readiness is required",
            "run `tentgent runtime bootstrap --profile training`",
            input,
        ),
        profile_backend(
            BackendKind::Embedding,
            "embedding backend readiness is not implemented yet",
            "install the future embedding runtime profile before using local embeddings",
            input,
        ),
        profile_backend(
            BackendKind::Rerank,
            "rerank backend readiness is not implemented yet",
            "install the future rerank runtime profile before using local rerank",
            input,
        ),
    ]
}

fn local_model_backend(backend: BackendKind, input: CapabilityProbeInput) -> BackendCapability {
    profile_backend(
        backend,
        "local-model profile readiness is required",
        "run `tentgent runtime bootstrap --profile local-model`",
        input,
    )
}

fn mlx_backend(
    os: &OperatingSystem,
    arch: &Architecture,
    gpu: &GpuFacts,
    input: CapabilityProbeInput,
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
            input,
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
    input: CapabilityProbeInput,
) -> BackendCapability {
    BackendCapability {
        backend,
        state: CapabilityState::Unknown,
        message: Some(if input.include_heavy_checks {
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

    use crate::foundation::layout::domain::{LayoutResolveMode, RuntimeLayout};
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, MetalFacts, OperatingSystem, PlatformFacts,
    };

    use super::*;

    #[derive(Debug)]
    struct FakePlatformProbe {
        facts: PlatformFacts,
    }

    impl PlatformProbe for FakePlatformProbe {
        fn query_platform_facts(&self) -> KernelResult<PlatformFacts> {
            Ok(self.facts.clone())
        }
    }

    #[derive(Debug)]
    struct FakeLayoutResolver {
        layout: RuntimeLayout,
    }

    impl RuntimeLayoutResolver for FakeLayoutResolver {
        fn resolve_runtime_layout(&self, mode: LayoutResolveMode) -> KernelResult<RuntimeLayout> {
            assert_eq!(mode, LayoutResolveMode::ReadOnly);
            Ok(self.layout.clone())
        }
    }

    #[test]
    fn probe_returns_report_not_manifest() {
        let probe = StdCapabilityProbe::new(
            FakePlatformProbe {
                facts: platform_facts(),
            },
            FakeLayoutResolver {
                layout: runtime_layout("/tmp/tentgent-capability-probe"),
            },
        );

        let report = probe
            .probe(CapabilityProbeInput {
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
