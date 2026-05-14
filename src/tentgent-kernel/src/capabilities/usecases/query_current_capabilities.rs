//! Read the current machine capability manifest.

use crate::capabilities::manifest::MachineCapabilityManifest;
use crate::capabilities::service::CapabilityRead;
use crate::foundation::error::KernelResult;

pub struct QueryCurrentCapabilities<'a, C> {
    pub capabilities: &'a C,
}

impl<'a, C> QueryCurrentCapabilities<'a, C> {
    pub fn new(capabilities: &'a C) -> Self {
        Self { capabilities }
    }
}

impl<C> QueryCurrentCapabilities<'_, C>
where
    C: CapabilityRead,
{
    pub fn run(&self) -> KernelResult<MachineCapabilityManifest> {
        self.capabilities.current()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::capabilities::domain::{BackendKind, CapabilityCheck, RuntimeCapabilityState};
    use crate::capabilities::manifest::{CapabilityManifestSchema, MachineCapabilityManifest};
    use crate::capabilities::service::CapabilityRead;
    use crate::features::runtime::domain::BootstrapProfile;
    use crate::foundation::error::KernelResult;
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
    };

    use super::QueryCurrentCapabilities;

    #[derive(Debug)]
    struct FakeCapabilityRead {
        manifest: MachineCapabilityManifest,
    }

    impl CapabilityRead for FakeCapabilityRead {
        fn current(&self) -> KernelResult<MachineCapabilityManifest> {
            Ok(self.manifest.clone())
        }

        fn check_profile(&self, _profile: BootstrapProfile) -> KernelResult<CapabilityCheck> {
            unreachable!("query current capabilities only reads current manifest")
        }

        fn check_backend(&self, _backend: BackendKind) -> KernelResult<CapabilityCheck> {
            unreachable!("query current capabilities only reads current manifest")
        }

        fn ensure_profile_ready(&self, _profile: BootstrapProfile) -> KernelResult<()> {
            unreachable!("query current capabilities only reads current manifest")
        }

        fn ensure_backend_ready(&self, _backend: BackendKind) -> KernelResult<()> {
            unreachable!("query current capabilities only reads current manifest")
        }
    }

    #[test]
    fn reads_current_manifest_through_capability_service() {
        let manifest = manifest();
        let service = FakeCapabilityRead {
            manifest: manifest.clone(),
        };

        let output = QueryCurrentCapabilities::new(&service)
            .run()
            .expect("query current manifest");

        assert_eq!(output, manifest);
    }

    fn manifest() -> MachineCapabilityManifest {
        MachineCapabilityManifest {
            schema: CapabilityManifestSchema {
                name: "tentgent.capabilities".to_string(),
                version: 1,
            },
            generated_at: None,
            platform: PlatformFacts {
                os: OperatingSystem::Linux,
                arch: Architecture::X86_64,
                libc: None,
                cpu: CpuFacts {
                    vendor: None,
                    brand: None,
                    features: Vec::new(),
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
                profiles: Vec::new(),
            },
            backends: Vec::new(),
        }
    }
}
