use crate::{
    features::resource_guard::{probes::server_spec_blockers, ResourceBlocker, ResourceOperation},
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

pub(crate) fn validate(
    context: &crate::features::resource_guard::probes::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    server_spec_blockers(context, layout, operation)
}
