use crate::{
    features::resource_coordination::{
        ResourceBusy, ResourceCoordinator, ResourceLockRequest, ResourcePermit,
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

pub fn acquire_or_busy(
    coordinator: &dyn ResourceCoordinator,
    layout: &RuntimeLayout,
    request: ResourceLockRequest,
) -> KernelResult<Result<ResourcePermit, ResourceBusy>> {
    coordinator.acquire(layout, request)
}
