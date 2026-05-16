//! Capability use case ports.

use crate::capabilities::domain::{BackendKind, MachineCapabilities};
use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;

use super::resolver::{MachineCapabilitiesInput, MachineCapabilitiesSnapshot};

/// Use-case boundary for reading or refreshing machine capability state.
pub trait MachineCapabilitiesResolver {
    /// Returns the cached capability snapshot when available, without forcing a probe.
    fn current(&self, input: MachineCapabilitiesInput)
        -> KernelResult<MachineCapabilitiesSnapshot>;

    /// Reprobes machine capabilities and persists the resulting snapshot.
    fn refresh(&self, input: MachineCapabilitiesInput)
        -> KernelResult<MachineCapabilitiesSnapshot>;
}

/// Use-case boundary for rejecting work that requires unavailable capabilities.
pub trait CapabilityGate {
    /// Fails when the requested backend is not supported by the current capability snapshot.
    fn ensure_backend(
        &self,
        capabilities: &MachineCapabilities,
        backend: BackendKind,
    ) -> KernelResult<()>;

    /// Fails when the requested runtime bootstrap profile is not supported locally.
    fn ensure_runtime_profile(
        &self,
        capabilities: &MachineCapabilities,
        profile: BootstrapProfile,
    ) -> KernelResult<()>;
}
