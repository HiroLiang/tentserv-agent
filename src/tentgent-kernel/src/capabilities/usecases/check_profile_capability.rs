//! Check one runtime profile capability.

use crate::capabilities::domain::CapabilityCheck;
use crate::capabilities::service::CapabilityRead;
use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;

pub struct CheckProfileCapability<'a> {
    capabilities: &'a dyn CapabilityRead,
}

impl<'a> CheckProfileCapability<'a> {
    pub fn new(capabilities: &'a dyn CapabilityRead) -> Self {
        Self { capabilities }
    }

    pub fn run(&self, profile: BootstrapProfile) -> KernelResult<CapabilityCheck> {
        self.capabilities.check_profile(profile)
    }
}

#[cfg(test)]
mod tests {
    use crate::capabilities::domain::{BackendKind, CapabilityCheck, CapabilityState};
    use crate::capabilities::manifest::MachineCapabilityManifest;
    use crate::capabilities::service::CapabilityRead;
    use crate::capabilities::usecases::CheckProfileCapability;
    use crate::features::runtime::domain::BootstrapProfile;
    use crate::foundation::error::KernelResult;

    #[derive(Debug)]
    struct FakeCapabilityRead;

    impl CapabilityRead for FakeCapabilityRead {
        fn current(&self) -> KernelResult<MachineCapabilityManifest> {
            unreachable!("check profile only checks profile capability")
        }

        fn check_profile(&self, profile: BootstrapProfile) -> KernelResult<CapabilityCheck> {
            assert_eq!(profile, BootstrapProfile::Training);
            Ok(CapabilityCheck {
                state: CapabilityState::Missing,
                message: Some("training profile is missing".to_string()),
                next_step: Some("run bootstrap training".to_string()),
            })
        }

        fn check_backend(&self, _backend: BackendKind) -> KernelResult<CapabilityCheck> {
            unreachable!("check profile only checks profile capability")
        }

        fn ensure_profile_ready(&self, _profile: BootstrapProfile) -> KernelResult<()> {
            unreachable!("check profile only checks profile capability")
        }

        fn ensure_backend_ready(&self, _backend: BackendKind) -> KernelResult<()> {
            unreachable!("check profile only checks profile capability")
        }
    }

    #[test]
    fn delegates_profile_check_to_capability_service() {
        let output = CheckProfileCapability::new(&FakeCapabilityRead)
            .run(BootstrapProfile::Training)
            .expect("check profile");

        assert_eq!(output.state, CapabilityState::Missing);
        assert_eq!(output.next_step.as_deref(), Some("run bootstrap training"));
    }
}
