//! Refresh the machine capability manifest.

use crate::foundation::error::KernelResult;

use crate::capabilities::manifest::{CapabilityManifestSchema, MachineCapabilityManifest};
use crate::capabilities::probes::{CapabilityProbe, CapabilityProbeInput};
use crate::capabilities::store::CapabilityManifestStore;
use crate::foundation::layout::usecases::query_runtime_layout::RuntimeLayoutQuery;
use crate::foundation::platform::usecases::query_platform_facts::PlatformFactsQuery;

pub struct RefreshCapabilities<'a> {
    pub query_platform_facts: &'a dyn PlatformFactsQuery,
    pub query_runtime_layout: &'a dyn RuntimeLayoutQuery,
    pub capability_probe: &'a dyn CapabilityProbe,
    pub store: &'a dyn CapabilityManifestStore,
    pub include_heavy_checks: bool,
}

impl<'a> RefreshCapabilities<'a> {
    pub fn new(
        query_platform_facts: &'a dyn PlatformFactsQuery,
        query_runtime_layout: &'a dyn RuntimeLayoutQuery,
        capability_probe: &'a dyn CapabilityProbe,
        store: &'a dyn CapabilityManifestStore,
        include_heavy_checks: bool,
    ) -> Self {
        Self {
            query_platform_facts,
            query_runtime_layout,
            capability_probe,
            store,
            include_heavy_checks,
        }
    }

    pub fn run(&self) -> KernelResult<MachineCapabilityManifest> {
        let platform = self.query_platform_facts.run()?;
        let layout = self.query_runtime_layout.run()?;
        let report = self.capability_probe.probe(CapabilityProbeInput {
            platform,
            layout,
            include_heavy_checks: self.include_heavy_checks,
        })?;
        let manifest = MachineCapabilityManifest {
            schema: CapabilityManifestSchema {
                name: "tentgent.capabilities".to_string(),
                version: 1,
            },
            generated_at: None,
            platform: report.platform,
            runtime: report.runtime,
            backends: report.backends,
        };

        self.store.save(&manifest)?;

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use crate::capabilities::domain::{
        BackendCapability, BackendKind, CapabilityProbeReport, CapabilityState,
        RuntimeCapabilityState, RuntimeProfileCapability,
    };
    use crate::capabilities::manifest::MachineCapabilityManifest;
    use crate::capabilities::probes::{CapabilityProbe, CapabilityProbeInput};
    use crate::capabilities::store::CapabilityManifestStore;
    use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
    use crate::foundation::error::KernelResult;
    use crate::foundation::layout::domain::{LayoutResolveMode, RuntimeLayout};
    use crate::foundation::layout::resolver::RuntimeLayoutResolver;
    use crate::foundation::layout::usecases::query_runtime_layout::QueryRuntimeLayout;
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
    };
    use crate::foundation::platform::probe::PlatformProbe;
    use crate::foundation::platform::usecases::query_platform_facts::QueryPlatformFacts;

    use super::RefreshCapabilities;

    #[derive(Debug)]
    struct FakeCapabilityProbe {
        seen_home_dir: RefCell<Option<PathBuf>>,
    }

    impl CapabilityProbe for FakeCapabilityProbe {
        fn probe(&self, input: CapabilityProbeInput) -> KernelResult<CapabilityProbeReport> {
            self.seen_home_dir
                .replace(Some(input.layout.home_dir.clone()));

            Ok(CapabilityProbeReport {
                platform: input.platform,
                runtime: RuntimeCapabilityState {
                    home_dir: input.layout.home_dir.clone(),
                    python_env_dir: input.layout.python_env_dir.clone(),
                    profiles: vec![RuntimeProfileCapability {
                        profile: BootstrapProfile::Base,
                        readiness: RuntimeReadiness::Ready,
                        message: None,
                        next_step: None,
                    }],
                },
                backends: vec![BackendCapability {
                    backend: BackendKind::CpuGguf,
                    state: CapabilityState::Unknown,
                    message: None,
                    next_step: None,
                }],
            })
        }
    }

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

    #[derive(Debug, Default)]
    struct FakeCapabilityStore {
        saved: RefCell<Option<MachineCapabilityManifest>>,
    }

    impl CapabilityManifestStore for FakeCapabilityStore {
        fn load(&self) -> KernelResult<Option<MachineCapabilityManifest>> {
            Ok(self.saved.borrow().clone())
        }

        fn save(&self, manifest: &MachineCapabilityManifest) -> KernelResult<()> {
            self.saved.replace(Some(manifest.clone()));
            Ok(())
        }
    }

    #[test]
    fn refresh_composes_manifest_from_probe_report_and_saves_it() {
        let layout = runtime_layout("/tmp/tentgent-refresh-capabilities");
        let platform_probe = FakePlatformProbe {
            facts: platform_facts(),
        };
        let layout_resolver = FakeLayoutResolver {
            layout: layout.clone(),
        };
        let query_platform_facts = QueryPlatformFacts::new(&platform_probe);
        let query_runtime_layout = QueryRuntimeLayout::new(&layout_resolver);
        let probe = FakeCapabilityProbe {
            seen_home_dir: RefCell::new(None),
        };
        let store = FakeCapabilityStore::default();

        let manifest = RefreshCapabilities::new(
            &query_platform_facts,
            &query_runtime_layout,
            &probe,
            &store,
            false,
        )
        .run()
        .expect("refresh capabilities");

        assert_eq!(manifest.schema.name, "tentgent.capabilities");
        assert_eq!(manifest.schema.version, 1);
        assert_eq!(manifest.platform.os, OperatingSystem::Linux);
        assert_eq!(
            probe.seen_home_dir.borrow().as_ref(),
            Some(&layout.home_dir)
        );
        assert_eq!(store.load().expect("load saved manifest"), Some(manifest));
    }

    fn platform_facts() -> PlatformFacts {
        PlatformFacts {
            os: OperatingSystem::Linux,
            arch: Architecture::X86_64,
            libc: None,
            cpu: CpuFacts {
                vendor: Some("GenuineIntel".to_string()),
                brand: Some("fixture cpu".to_string()),
                features: vec!["avx2".to_string()],
            },
            gpu: GpuFacts {
                devices: Vec::new(),
                cuda: None,
                metal: None,
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
