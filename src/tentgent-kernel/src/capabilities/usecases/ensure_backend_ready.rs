//! Ensure a backend is ready before starting work.

use crate::capabilities::domain::BackendKind;
use crate::capabilities::service::CapabilityRead;
use crate::foundation::error::KernelResult;

pub struct EnsureBackendReady<'a> {
    capabilities: &'a dyn CapabilityRead,
}

impl<'a> EnsureBackendReady<'a> {
    pub fn new(capabilities: &'a dyn CapabilityRead) -> Self {
        Self { capabilities }
    }

    pub fn run(&self, backend: BackendKind) -> KernelResult<()> {
        self.capabilities.ensure_backend_ready(backend)
    }
}

#[cfg(test)]
mod tests {
    use crate::capabilities::domain::{BackendKind, CapabilityCheck};
    use crate::capabilities::manifest::MachineCapabilityManifest;
    use crate::capabilities::service::CapabilityRead;
    use crate::capabilities::usecases::EnsureBackendReady;
    use crate::features::runtime::domain::BootstrapProfile;
    use crate::foundation::error::{KernelError, KernelResult};

    #[derive(Debug)]
    struct FakeCapabilityRead;

    impl CapabilityRead for FakeCapabilityRead {
        fn current(&self) -> KernelResult<MachineCapabilityManifest> {
            unreachable!("ensure backend only gates backend capability")
        }

        fn check_profile(&self, _profile: BootstrapProfile) -> KernelResult<CapabilityCheck> {
            unreachable!("ensure backend only gates backend capability")
        }

        fn check_backend(&self, _backend: BackendKind) -> KernelResult<CapabilityCheck> {
            unreachable!("ensure backend only gates backend capability")
        }

        fn ensure_profile_ready(&self, _profile: BootstrapProfile) -> KernelResult<()> {
            unreachable!("ensure backend only gates backend capability")
        }

        fn ensure_backend_ready(&self, backend: BackendKind) -> KernelResult<()> {
            assert_eq!(backend, BackendKind::CpuGguf);
            Err(KernelError::BackendCapabilityNotReady {
                backend: backend.name().to_string(),
                next_step: "install local-model profile".to_string(),
            })
        }
    }

    #[test]
    fn delegates_backend_gate_to_capability_service() {
        let err = EnsureBackendReady::new(&FakeCapabilityRead)
            .run(BackendKind::CpuGguf)
            .expect_err("cpu gguf backend should be rejected");

        assert!(matches!(
            err,
            KernelError::BackendCapabilityNotReady { backend, next_step }
                if backend == "cpu-gguf" && next_step == "install local-model profile"
        ));
    }
}
