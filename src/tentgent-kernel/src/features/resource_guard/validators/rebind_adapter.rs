use crate::{
    features::resource_guard::{
        probes::{ownership_blockers, server_spec_blockers, train_reference_blockers},
        ResourceBlocker, ResourceOperation,
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

pub(crate) fn validate(
    context: &crate::features::resource_guard::probes::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let mut blockers = server_spec_blockers(context, layout, operation)?;
    blockers.extend(train_reference_blockers(context, layout, operation)?);
    blockers.extend(ownership_blockers(context, layout, operation)?);
    Ok(blockers)
}
