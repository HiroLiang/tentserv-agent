use crate::foundation::{error::KernelResult, layout::RuntimeLayout};

use super::{ResourceMutationAuthorization, ResourceOperation};

pub trait ResourceGuardUseCase: Send + Sync {
    fn authorize(
        &self,
        layout: &RuntimeLayout,
        operation: ResourceOperation,
    ) -> KernelResult<ResourceMutationAuthorization>;
}
