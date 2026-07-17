use std::fs;

use crate::{
    features::{
        cluster::domain::{ClusterDefinition, ClusterRouteUpdatePolicy, ClusterStoreLayout},
        resource_guard::{
            blocker,
            probes::{ownership_blockers, server_spec_blockers},
            ResourceBlocker, ResourceBlockerCode, ResourceOperation,
        },
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

pub(crate) fn validate(
    context: &crate::features::resource_guard::probes::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let mut blockers = server_spec_blockers(context, layout, operation)?;
    blockers.extend(ownership_blockers(context, layout, operation)?);
    blockers.extend(route_update_policy_blockers(layout, operation)?);
    Ok(blockers)
}

fn route_update_policy_blockers(
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let ResourceOperation::ReplaceCluster {
        cluster_ref,
        definition: next,
    } = operation
    else {
        return Ok(Vec::new());
    };
    let store = ClusterStoreLayout::from_home_dir(layout.home_dir.clone());
    let path = store.cluster_definition_path(cluster_ref);
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(policy_probe_error(error)),
    };
    let current: ClusterDefinition = toml::from_str(&body).map_err(policy_probe_error)?;
    if current.routes == next.routes
        || current.route_update_policy != ClusterRouteUpdatePolicy::Block
    {
        return Ok(Vec::new());
    }

    let reason = if next.route_update_policy == ClusterRouteUpdatePolicy::Drain {
        "switch route_update_policy from block to drain in a policy-only apply before changing routes"
    } else {
        "stored route_update_policy is block; route targets cannot change"
    };
    let mut value = blocker(
        operation,
        "cluster-policy",
        ResourceBlockerCode::ClusterRouteUpdateBlocked,
        cluster_ref,
        reason,
    );
    value.field = Some("route_update_policy".to_string());
    value.next_actions = vec![
        "apply route_update_policy = \"drain\" without changing routes, then retry".to_string(),
    ];
    Ok(vec![value])
}

fn policy_probe_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(format!(
        "resource guard cluster policy probe failed: {error}"
    ))
}
