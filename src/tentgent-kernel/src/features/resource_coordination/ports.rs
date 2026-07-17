use crate::{foundation::error::KernelResult, foundation::layout::RuntimeLayout};

use super::{ResourceBusy, ResourceLockRequest, ResourcePermit};

pub trait ResourceCoordinationLease: Send + Sync {}

pub trait ResourceCoordinator: Send + Sync {
    fn acquire(
        &self,
        layout: &RuntimeLayout,
        request: ResourceLockRequest,
    ) -> KernelResult<Result<ResourcePermit, ResourceBusy>>;
}
