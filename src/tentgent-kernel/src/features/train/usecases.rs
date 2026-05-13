//! Training application workflows.

use crate::capabilities::service::CapabilityRead;
use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTrainingInput {
    pub plan_ref: String,
}

pub fn validate_run_training(
    capabilities: &impl CapabilityRead,
    _input: &RunTrainingInput,
) -> KernelResult<()> {
    capabilities.ensure_profile_ready(BootstrapProfile::Training)
}
