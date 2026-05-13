//! High-level capability interfaces consumed by feature use cases.

use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;

use super::domain::{BackendKind, CapabilityCheck};
use super::manifest::MachineCapabilityManifest;

pub trait CapabilityRead {
    fn current(&self) -> KernelResult<MachineCapabilityManifest>;
    fn check_profile(&self, profile: BootstrapProfile) -> KernelResult<CapabilityCheck>;
    fn check_backend(&self, backend: BackendKind) -> KernelResult<CapabilityCheck>;
    fn ensure_profile_ready(&self, profile: BootstrapProfile) -> KernelResult<()>;
    fn ensure_backend_ready(&self, backend: BackendKind) -> KernelResult<()>;
}
