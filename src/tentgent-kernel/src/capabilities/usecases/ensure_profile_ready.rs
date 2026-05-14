//! Ensure a runtime profile is ready before starting work.

use crate::capabilities::service::CapabilityRead;
use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;

pub struct EnsureProfileReady<'a> {
    capabilities: &'a dyn CapabilityRead,
}

impl<'a> EnsureProfileReady<'a> {
    pub fn new(capabilities: &'a dyn CapabilityRead) -> Self {
        Self { capabilities }
    }

    pub fn run(&self, profile: BootstrapProfile) -> KernelResult<()> {
        self.capabilities.ensure_profile_ready(profile)
    }
}

#[cfg(test)]
mod tests {
    use crate::capabilities::domain::{BackendKind, CapabilityCheck};
    use crate::capabilities::manifest::MachineCapabilityManifest;
    use crate::capabilities::service::CapabilityRead;
    use crate::capabilities::usecases::EnsureProfileReady;
    use crate::features::runtime::domain::BootstrapProfile;
    use crate::foundation::error::{KernelError, KernelResult};

    #[derive(Debug)]
    struct FakeCapabilityRead;

    impl CapabilityRead for FakeCapabilityRead {
        fn current(&self) -> KernelResult<MachineCapabilityManifest> {
            unreachable!("ensure profile only gates profile capability")
        }

        fn check_profile(&self, _profile: BootstrapProfile) -> KernelResult<CapabilityCheck> {
            unreachable!("ensure profile only gates profile capability")
        }

        fn check_backend(&self, _backend: BackendKind) -> KernelResult<CapabilityCheck> {
            unreachable!("ensure profile only gates profile capability")
        }

        fn ensure_profile_ready(&self, profile: BootstrapProfile) -> KernelResult<()> {
            assert_eq!(profile, BootstrapProfile::Training);
            Err(KernelError::RuntimeProfileNotReady {
                profile: profile.name().to_string(),
                next_step: "run bootstrap training".to_string(),
            })
        }

        fn ensure_backend_ready(&self, _backend: BackendKind) -> KernelResult<()> {
            unreachable!("ensure profile only gates profile capability")
        }
    }

    #[test]
    fn delegates_profile_gate_to_capability_service() {
        let err = EnsureProfileReady::new(&FakeCapabilityRead)
            .run(BootstrapProfile::Training)
            .expect_err("training profile should be rejected");

        assert!(matches!(
            err,
            KernelError::RuntimeProfileNotReady { profile, next_step }
                if profile == "training" && next_step == "run bootstrap training"
        ));
    }
}
