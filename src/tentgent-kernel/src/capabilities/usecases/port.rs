//! Capability use case ports.

use crate::capabilities::domain::{BackendKind, MachineCapabilities};
use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;

use super::resolver::{MachineCapabilitiesInput, MachineCapabilitiesSnapshot};

pub trait MachineCapabilitiesResolver {
    fn current(&self, input: MachineCapabilitiesInput)
        -> KernelResult<MachineCapabilitiesSnapshot>;

    fn refresh(&self, input: MachineCapabilitiesInput)
        -> KernelResult<MachineCapabilitiesSnapshot>;
}

pub trait CapabilityGate {
    fn ensure_backend(
        &self,
        capabilities: &MachineCapabilities,
        backend: BackendKind,
    ) -> KernelResult<()>;

    fn ensure_runtime_profile(
        &self,
        capabilities: &MachineCapabilities,
        profile: BootstrapProfile,
    ) -> KernelResult<()>;
}
