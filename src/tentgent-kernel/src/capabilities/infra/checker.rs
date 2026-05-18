//! Capability state checker implementation.

use crate::capabilities::domain::{
    BackendKind, CapabilityCheck, CapabilityState, MachineCapabilities, RuntimeProfileCapability,
};
use crate::capabilities::ports::CapabilityChecker;
use crate::features::runtime::domain::{BootstrapProfile, RuntimeReadiness};
use crate::foundation::error::KernelResult;

#[derive(Debug, Clone, Copy, Default)]
pub struct StdCapabilityChecker;

impl CapabilityChecker for StdCapabilityChecker {
    fn check_backend(
        &self,
        capabilities: &MachineCapabilities,
        backend: BackendKind,
    ) -> KernelResult<CapabilityCheck> {
        Ok(capabilities
            .backends
            .iter()
            .find(|candidate| candidate.backend == backend)
            .map(|capability| CapabilityCheck {
                state: capability.state,
                message: capability.message.clone(),
                next_step: capability.next_step.clone(),
            })
            .unwrap_or_else(|| missing_capability("backend capability is missing from state")))
    }

    fn check_runtime_profile(
        &self,
        capabilities: &MachineCapabilities,
        profile: BootstrapProfile,
    ) -> KernelResult<CapabilityCheck> {
        Ok(capabilities
            .runtime
            .profiles
            .iter()
            .find(|candidate| candidate.profile == profile)
            .map(runtime_profile_check)
            .unwrap_or_else(|| missing_capability("runtime profile is missing from state")))
    }
}

fn runtime_profile_check(profile: &RuntimeProfileCapability) -> CapabilityCheck {
    CapabilityCheck {
        state: runtime_readiness_to_capability_state(profile.readiness),
        message: profile.message.clone(),
        next_step: profile.next_step.clone(),
    }
}

fn runtime_readiness_to_capability_state(readiness: RuntimeReadiness) -> CapabilityState {
    match readiness {
        RuntimeReadiness::Ready => CapabilityState::Ready,
        RuntimeReadiness::Missing => CapabilityState::Missing,
        RuntimeReadiness::Stale => CapabilityState::Stale,
        RuntimeReadiness::Unsupported => CapabilityState::Unsupported,
        RuntimeReadiness::Unknown => CapabilityState::Unknown,
    }
}

fn missing_capability(message: &str) -> CapabilityCheck {
    CapabilityCheck {
        state: CapabilityState::Unknown,
        message: Some(message.to_string()),
        next_step: Some("refresh capability state".to_string()),
    }
}
