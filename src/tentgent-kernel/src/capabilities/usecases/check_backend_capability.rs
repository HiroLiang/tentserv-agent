//! Check one backend capability.

use crate::capabilities::domain::{BackendKind, CapabilityCheck};
use crate::capabilities::service::CapabilityRead;
use crate::foundation::error::KernelResult;

pub struct CheckBackendCapability<'a> {
    capabilities: &'a dyn CapabilityRead,
}

impl<'a> CheckBackendCapability<'a> {
    pub fn new(capabilities: &'a dyn CapabilityRead) -> Self {
        Self { capabilities }
    }

    pub fn run(&self, backend: BackendKind) -> KernelResult<CapabilityCheck> {
        self.capabilities.check_backend(backend)
    }
}

#[cfg(test)]
mod tests {
    use crate::capabilities::domain::{BackendKind, CapabilityCheck, CapabilityState};
    use crate::capabilities::manifest::MachineCapabilityManifest;
    use crate::capabilities::service::CapabilityRead;
    use crate::capabilities::usecases::CheckBackendCapability;
    use crate::features::runtime::domain::BootstrapProfile;
    use crate::foundation::error::KernelResult;

    #[derive(Debug)]
    struct FakeCapabilityRead;

    impl CapabilityRead for FakeCapabilityRead {
        fn current(&self) -> KernelResult<MachineCapabilityManifest> {
            unreachable!("check backend only checks backend capability")
        }

        fn check_profile(&self, _profile: BootstrapProfile) -> KernelResult<CapabilityCheck> {
            unreachable!("check backend only checks backend capability")
        }

        fn check_backend(&self, backend: BackendKind) -> KernelResult<CapabilityCheck> {
            assert_eq!(backend, BackendKind::CpuGguf);
            Ok(CapabilityCheck {
                state: CapabilityState::Blocked,
                message: Some("cpu gguf backend is blocked".to_string()),
                next_step: Some("install local-model profile".to_string()),
            })
        }

        fn ensure_profile_ready(&self, _profile: BootstrapProfile) -> KernelResult<()> {
            unreachable!("check backend only checks backend capability")
        }

        fn ensure_backend_ready(&self, _backend: BackendKind) -> KernelResult<()> {
            unreachable!("check backend only checks backend capability")
        }
    }

    #[test]
    fn delegates_backend_check_to_capability_service() {
        let output = CheckBackendCapability::new(&FakeCapabilityRead)
            .run(BackendKind::CpuGguf)
            .expect("check backend");

        assert_eq!(output.state, CapabilityState::Blocked);
        assert_eq!(
            output.next_step.as_deref(),
            Some("install local-model profile")
        );
    }
}
