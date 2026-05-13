//! Capability manifest persistence boundary.

use crate::foundation::error::KernelResult;

use super::manifest::MachineCapabilityManifest;

pub trait CapabilityManifestStore {
    fn load(&self) -> KernelResult<Option<MachineCapabilityManifest>>;
    fn save(&self, manifest: &MachineCapabilityManifest) -> KernelResult<()>;
}
