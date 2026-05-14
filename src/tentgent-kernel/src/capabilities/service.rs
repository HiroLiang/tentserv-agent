//! High-level capability interfaces consumed by feature use cases.

use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::{KernelError, KernelResult};

use super::domain::{BackendKind, CapabilityCheck};
use super::manifest::MachineCapabilityManifest;
use super::store::CapabilityManifestStore;

pub trait CapabilityRead {
    fn current(&self) -> KernelResult<MachineCapabilityManifest>;
    fn check_profile(&self, profile: BootstrapProfile) -> KernelResult<CapabilityCheck>;
    fn check_backend(&self, backend: BackendKind) -> KernelResult<CapabilityCheck>;
    fn ensure_profile_ready(&self, profile: BootstrapProfile) -> KernelResult<()>;
    fn ensure_backend_ready(&self, backend: BackendKind) -> KernelResult<()>;
}

pub struct StoreCapabilityService<'a, S> {
    pub store: &'a S,
}

impl<'a, S> StoreCapabilityService<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }
}

impl<S> CapabilityRead for StoreCapabilityService<'_, S>
where
    S: CapabilityManifestStore,
{
    fn current(&self) -> KernelResult<MachineCapabilityManifest> {
        self.store.load()?.ok_or_else(|| {
            KernelError::CapabilityManifestUnavailable(
                "missing; refresh capabilities before reading current state".to_string(),
            )
        })
    }

    fn check_profile(&self, profile: BootstrapProfile) -> KernelResult<CapabilityCheck> {
        let manifest = self.current()?;
        let capability = manifest
            .runtime
            .profiles
            .iter()
            .find(|capability| capability.profile == profile);

        Ok(match capability {
            Some(capability) => CapabilityCheck {
                state: capability.readiness.into(),
                message: capability.message.clone(),
                next_step: capability.next_step.clone(),
            },
            None => CapabilityCheck {
                state: super::domain::CapabilityState::Unknown,
                message: Some(format!(
                    "runtime profile capability is missing from manifest: {}",
                    profile.name()
                )),
                next_step: Some("refresh capabilities".to_string()),
            },
        })
    }

    fn check_backend(&self, backend: BackendKind) -> KernelResult<CapabilityCheck> {
        let manifest = self.current()?;
        let capability = manifest
            .backends
            .iter()
            .find(|capability| capability.backend == backend);

        Ok(match capability {
            Some(capability) => CapabilityCheck {
                state: capability.state,
                message: capability.message.clone(),
                next_step: capability.next_step.clone(),
            },
            None => CapabilityCheck {
                state: super::domain::CapabilityState::Unknown,
                message: Some(format!(
                    "backend capability is missing from manifest: {}",
                    backend.name()
                )),
                next_step: Some("refresh capabilities".to_string()),
            },
        })
    }

    fn ensure_profile_ready(&self, profile: BootstrapProfile) -> KernelResult<()> {
        let check = self.check_profile(profile)?;
        if check.state == super::domain::CapabilityState::Ready {
            return Ok(());
        }

        Err(KernelError::RuntimeProfileNotReady {
            profile: profile.name().to_string(),
            next_step: check
                .next_step
                .or(check.message)
                .unwrap_or_else(|| "refresh capabilities".to_string()),
        })
    }

    fn ensure_backend_ready(&self, backend: BackendKind) -> KernelResult<()> {
        let check = self.check_backend(backend)?;
        if check.state == super::domain::CapabilityState::Ready {
            return Ok(());
        }

        Err(KernelError::BackendCapabilityNotReady {
            backend: backend.name().to_string(),
            next_step: check
                .next_step
                .or(check.message)
                .unwrap_or_else(|| "refresh capabilities".to_string()),
        })
    }
}

impl From<crate::features::runtime::domain::RuntimeReadiness> for super::domain::CapabilityState {
    fn from(readiness: crate::features::runtime::domain::RuntimeReadiness) -> Self {
        match readiness {
            crate::features::runtime::domain::RuntimeReadiness::Ready => Self::Ready,
            crate::features::runtime::domain::RuntimeReadiness::Missing => Self::Missing,
            crate::features::runtime::domain::RuntimeReadiness::Stale => Self::Stale,
            crate::features::runtime::domain::RuntimeReadiness::Unsupported => Self::Unsupported,
            crate::features::runtime::domain::RuntimeReadiness::Unknown => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use crate::capabilities::domain::{
        BackendCapability, BackendKind, CapabilityState, RuntimeCapabilityState,
        RuntimeProfileCapability,
    };
    use crate::capabilities::manifest::{CapabilityManifestSchema, MachineCapabilityManifest};
    use crate::capabilities::service::{CapabilityRead, StoreCapabilityService};
    use crate::capabilities::store::CapabilityManifestStore;
    use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
    use crate::foundation::error::{KernelError, KernelResult};
    use crate::foundation::platform::domain::{
        Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
    };

    #[derive(Debug, Default)]
    struct FakeStore {
        manifest: RefCell<Option<MachineCapabilityManifest>>,
    }

    impl FakeStore {
        fn with_manifest(manifest: MachineCapabilityManifest) -> Self {
            Self {
                manifest: RefCell::new(Some(manifest)),
            }
        }
    }

    impl CapabilityManifestStore for FakeStore {
        fn load(&self) -> KernelResult<Option<MachineCapabilityManifest>> {
            Ok(self.manifest.borrow().clone())
        }

        fn save(&self, manifest: &MachineCapabilityManifest) -> KernelResult<()> {
            self.manifest.replace(Some(manifest.clone()));
            Ok(())
        }
    }

    #[test]
    fn current_reads_manifest_from_store() {
        let manifest = manifest(RuntimeReadiness::Ready, CapabilityState::Ready);
        let store = FakeStore::with_manifest(manifest.clone());
        let service = StoreCapabilityService::new(&store);

        assert_eq!(service.current().expect("current manifest"), manifest);
    }

    #[test]
    fn current_missing_manifest_returns_actionable_error() {
        let store = FakeStore::default();
        let service = StoreCapabilityService::new(&store);

        let err = service.current().expect_err("missing manifest should fail");

        assert!(matches!(
            err,
            KernelError::CapabilityManifestUnavailable(message)
                if message.contains("refresh capabilities")
        ));
    }

    #[test]
    fn ensure_profile_ready_rejects_missing_profile() {
        let store =
            FakeStore::with_manifest(manifest(RuntimeReadiness::Missing, CapabilityState::Ready));
        let service = StoreCapabilityService::new(&store);

        let err = service
            .ensure_profile_ready(BootstrapProfile::Base)
            .expect_err("missing profile should fail");

        assert!(matches!(
            err,
            KernelError::RuntimeProfileNotReady { profile, next_step }
                if profile == "base"
                    && next_step == "run `tentgent runtime bootstrap --profile base`"
        ));
    }

    #[test]
    fn ensure_backend_ready_rejects_blocked_backend() {
        let store =
            FakeStore::with_manifest(manifest(RuntimeReadiness::Ready, CapabilityState::Blocked));
        let service = StoreCapabilityService::new(&store);

        let err = service
            .ensure_backend_ready(BackendKind::CpuGguf)
            .expect_err("blocked backend should fail");

        assert!(matches!(
            err,
            KernelError::BackendCapabilityNotReady { backend, next_step }
                if backend == "cpu-gguf" && next_step == "install local-model profile"
        ));
    }

    fn manifest(
        base_readiness: RuntimeReadiness,
        backend_state: CapabilityState,
    ) -> MachineCapabilityManifest {
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
                    brand: Some("fixture cpu".to_string()),
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
                profiles: vec![RuntimeProfileCapability {
                    profile: BootstrapProfile::Base,
                    readiness: base_readiness,
                    message: None,
                    next_step: Some("run `tentgent runtime bootstrap --profile base`".to_string()),
                }],
            },
            backends: vec![BackendCapability {
                backend: BackendKind::CpuGguf,
                state: backend_state,
                message: None,
                next_step: Some("install local-model profile".to_string()),
            }],
        }
    }
}
