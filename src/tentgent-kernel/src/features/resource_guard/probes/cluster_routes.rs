use std::fs;

use crate::{
    features::{
        cluster::domain::{ClusterDefinition, ClusterRouteTarget, CLUSTERS_DIRNAME},
        resource_guard::{blocker, ResourceBlocker, ResourceBlockerCode, ResourceOperation},
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

pub(crate) fn cluster_route_blockers(
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let root = layout.home_dir.join(CLUSTERS_DIRNAME);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut blockers = Vec::new();
    for entry in fs::read_dir(root).map_err(guard_error)? {
        let path = entry.map_err(guard_error)?.path().join("cluster.toml");
        if !path.exists() {
            continue;
        }
        let definition: ClusterDefinition =
            toml::from_str(&fs::read_to_string(path).map_err(guard_error)?).map_err(guard_error)?;
        for (route, target) in &definition.routes {
            let ClusterRouteTarget::LocalModel { model_ref, .. } = target else {
                continue;
            };
            let matches = match operation {
                ResourceOperation::DeleteModel {
                    model_ref: expected,
                } => model_ref.as_str() == expected,
                ResourceOperation::RemoveModelCapability {
                    model_ref: expected,
                    capability,
                } => model_ref.as_str() == expected && route.model_capability() == *capability,
                ResourceOperation::ReplaceModelCapabilities {
                    model_ref: expected,
                    removed_capabilities,
                } => {
                    model_ref.as_str() == expected
                        && removed_capabilities.contains(&route.model_capability())
                }
                _ => false,
            };
            if matches {
                let mut value = blocker(
                    operation,
                    "cluster-route",
                    if matches!(operation, ResourceOperation::DeleteModel { .. }) {
                        ResourceBlockerCode::ModelInUse
                    } else {
                        ResourceBlockerCode::CapabilityInUse
                    },
                    format!("{}:{route}", definition.cluster_ref),
                    "cluster route references this model capability",
                );
                value.owner = Some(definition.cluster_ref.to_string());
                value.route = Some(route.to_string());
                value.capability = Some(route.model_capability().to_string());
                value.field = Some("routes".to_string());
                blockers.push(value);
            }
        }
    }
    Ok(blockers)
}

fn guard_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(format!(
        "resource guard cluster probe failed: {error}"
    ))
}
