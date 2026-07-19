use crate::{
    features::resource_guard::{
        probes::train_reference_blockers, ResourceBlocker, ResourceOperation,
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

pub(crate) fn validate(
    context: &crate::features::resource_guard::probes::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    train_reference_blockers(context, layout, operation)
}
