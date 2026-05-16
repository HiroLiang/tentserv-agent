//! Capability package ports.

use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::PlatformFacts;

use super::domain::{BackendKind, CapabilityCheck, MachineCapabilities};

/// Probes local machine capability facts that may depend on files, tools, or hardware.
pub trait MachineCapabilitiesProbe {
    /// Produces a capability snapshot for the given runtime layout and platform facts.
    fn probe(
        &self,
        layout: &RuntimeLayout,
        platform: &PlatformFacts,
    ) -> KernelResult<MachineCapabilities>;
}

/// Persists and reloads the cached capability snapshot for a runtime home.
pub trait CapabilityStateStore {
    /// Loads the last recorded capability state, if one exists.
    fn load(&self, layout: &RuntimeLayout) -> KernelResult<Option<MachineCapabilities>>;

    /// Saves the current capability state so later checks can avoid reprobing.
    fn save(&self, layout: &RuntimeLayout, capabilities: &MachineCapabilities) -> KernelResult<()>;
}

/// Converts a capability snapshot into readiness checks for requested runtime features.
pub trait CapabilityChecker {
    /// Checks whether a specific model backend can run on the current machine.
    fn check_backend(
        &self,
        capabilities: &MachineCapabilities,
        backend: BackendKind,
    ) -> KernelResult<CapabilityCheck>;

    /// Checks whether a runtime bootstrap profile is compatible with the current machine.
    fn check_runtime_profile(
        &self,
        capabilities: &MachineCapabilities,
        profile: BootstrapProfile,
    ) -> KernelResult<CapabilityCheck>;
}
