//! Capability gate use case.

use crate::capabilities::domain::{
    BackendKind, CapabilityCheck, CapabilityState, MachineCapabilities,
};
use crate::capabilities::ports::CapabilityChecker;
use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::{KernelError, KernelResult};

use super::port::CapabilityGate;

pub struct StdCapabilityGate<'a> {
    checker: &'a dyn CapabilityChecker,
}

impl<'a> StdCapabilityGate<'a> {
    pub fn new(checker: &'a dyn CapabilityChecker) -> Self {
        Self { checker }
    }
}

impl CapabilityGate for StdCapabilityGate<'_> {
    fn ensure_backend(
        &self,
        capabilities: &MachineCapabilities,
        backend: BackendKind,
    ) -> KernelResult<()> {
        let check = self.checker.check_backend(capabilities, backend)?;
        if check.state == CapabilityState::Ready {
            return Ok(());
        }

        Err(KernelError::BackendCapabilityNotReady {
            backend: format!("{backend:?}"),
            next_step: next_step(&check),
        })
    }

    fn ensure_runtime_profile(
        &self,
        capabilities: &MachineCapabilities,
        profile: BootstrapProfile,
    ) -> KernelResult<()> {
        let check = self.checker.check_runtime_profile(capabilities, profile)?;
        if check.state == CapabilityState::Ready {
            return Ok(());
        }

        Err(KernelError::RuntimeProfileNotReady {
            profile: format!("{profile:?}"),
            next_step: next_step(&check),
        })
    }
}

fn next_step(check: &CapabilityCheck) -> String {
    check
        .next_step
        .clone()
        .or_else(|| check.message.clone())
        .unwrap_or_else(|| "refresh capability state".to_string())
}
