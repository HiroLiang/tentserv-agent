//! Runtime bootstrap, init, status, and doctor workflows.

use crate::foundation::error::KernelResult;

use super::domain::{BootstrapProfile, RuntimeInitState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapRuntimeInput {
    pub profile: BootstrapProfile,
    pub dry_run: bool,
    pub print_plan: bool,
}

pub trait RuntimeStateRead {
    fn inspect_runtime(&self) -> KernelResult<RuntimeInitState>;
    fn ensure_profile_ready(&self, profile: BootstrapProfile) -> KernelResult<()>;
}
