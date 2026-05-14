//! Training application workflows.

use crate::capabilities::usecases::EnsureProfileReady;
use crate::features::runtime::domain::BootstrapProfile;
use crate::foundation::error::KernelResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTrainingInput {
    pub plan_ref: String,
}

pub fn validate_run_training(
    ensure_profile_ready: &EnsureProfileReady<'_>,
    _input: &RunTrainingInput,
) -> KernelResult<()> {
    ensure_profile_ready.run(BootstrapProfile::Training)
}
