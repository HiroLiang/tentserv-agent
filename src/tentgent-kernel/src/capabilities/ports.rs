//! Capability package ports.

use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;
use crate::foundation::platform::PlatformFacts;

use super::domain::{BackendKind, CapabilityCheck, MachineCapabilities};
use super::usecases::{MachineCapabilitiesInput, MachineCapabilitiesSnapshot};

pub trait MachineCapabilitiesProbe {
    fn probe(
        &self,
        layout: &RuntimeLayout,
        platform: &PlatformFacts,
    ) -> KernelResult<MachineCapabilities>;
}

pub trait CapabilityStateStore {
    fn load(&self, layout: &RuntimeLayout) -> KernelResult<Option<MachineCapabilities>>;

    fn save(&self, layout: &RuntimeLayout, capabilities: &MachineCapabilities) -> KernelResult<()>;
}

pub trait MachineCapabilitiesResolver {
    fn current(&self, input: MachineCapabilitiesInput)
        -> KernelResult<MachineCapabilitiesSnapshot>;

    fn refresh(&self, input: MachineCapabilitiesInput)
        -> KernelResult<MachineCapabilitiesSnapshot>;
}

pub trait CapabilityChecker {
    fn check_backend(
        &self,
        capabilities: &MachineCapabilities,
        backend: BackendKind,
    ) -> KernelResult<CapabilityCheck>;

    fn check_runtime_profile(
        &self,
        capabilities: &MachineCapabilities,
        profile: BootstrapProfile,
    ) -> KernelResult<CapabilityCheck>;
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
