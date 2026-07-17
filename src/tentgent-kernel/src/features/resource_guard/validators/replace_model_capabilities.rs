use crate::{
    features::resource_guard::{
        probes::{
            adapter_binding_blockers, cluster_route_blockers, ownership_blockers,
            server_spec_blockers,
        },
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
    blockers.extend(cluster_route_blockers(layout, operation)?);
    blockers.extend(adapter_binding_blockers(layout, operation)?);
    blockers.extend(ownership_blockers(context, layout, operation)?);
    Ok(blockers)
}
