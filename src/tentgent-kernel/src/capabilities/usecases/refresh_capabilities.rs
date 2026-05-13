//! Refresh the machine capability manifest.

use crate::foundation::error::KernelResult;

use crate::capabilities::manifest::{CapabilityManifestSchema, MachineCapabilityManifest};
use crate::capabilities::probes::{CapabilityProbe, CapabilityProbeInput};
use crate::capabilities::store::CapabilityManifestStore;

pub struct RefreshCapabilities<'a, P, S> {
    pub probe: &'a P,
    pub store: &'a S,
    pub input: CapabilityProbeInput,
}

impl<'a, P, S> RefreshCapabilities<'a, P, S> {
    pub fn new(probe: &'a P, store: &'a S, input: CapabilityProbeInput) -> Self {
        Self {
            probe,
            store,
            input,
        }
    }
}

impl<P, S> RefreshCapabilities<'_, P, S>
where
    P: CapabilityProbe,
    S: CapabilityManifestStore,
{
    pub fn run(&self) -> KernelResult<MachineCapabilityManifest> {
        let report = self.probe.probe(self.input)?;
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
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
    };

    use super::RefreshCapabilities;

    #[derive(Debug)]
    struct FakeCapabilityProbe {
        report: CapabilityProbeReport,
    }

    impl CapabilityProbe for FakeCapabilityProbe {
        fn probe(&self, _input: CapabilityProbeInput) -> KernelResult<CapabilityProbeReport> {
            Ok(self.report.clone())
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
        let probe = FakeCapabilityProbe {
            report: probe_report(),
        };
        let store = FakeCapabilityStore::default();

        let manifest = RefreshCapabilities::new(
            &probe,
            &store,
            CapabilityProbeInput {
                include_heavy_checks: false,
            },
        )
        .run()
        .expect("refresh capabilities");

        assert_eq!(manifest.schema.name, "tentgent.capabilities");
        assert_eq!(manifest.schema.version, 1);
        assert_eq!(manifest.platform.os, OperatingSystem::Linux);
        assert_eq!(store.load().expect("load saved manifest"), Some(manifest));
    }

    fn probe_report() -> CapabilityProbeReport {
        CapabilityProbeReport {
            platform: PlatformFacts {
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
            },
            runtime: RuntimeCapabilityState {
                home_dir: PathBuf::from("/tmp/tentgent"),
                python_env_dir: PathBuf::from("/tmp/tentgent/runtime/python-env"),
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
        }
    }
}
