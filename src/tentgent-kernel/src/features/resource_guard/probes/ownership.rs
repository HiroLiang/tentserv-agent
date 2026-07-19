use crate::{
    features::{
        resource_guard::{blocker, ResourceBlocker, ResourceBlockerCode, ResourceOperation},
        runtime::infra::ModelRuntimeCapability,
        runtime_ownership::{RuntimeExecutionIdentity, RuntimeOwnershipLayout},
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

pub(crate) fn ownership_blockers(
    context: &super::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
    let (claims, claim_issues) = context.ownership_store.list_claims(&ownership)?;
    let (generations, generation_issues) = context.ownership_store.list_generations(&ownership)?;
    let mut blockers = Vec::new();
    for issue in claim_issues.into_iter().chain(generation_issues) {
        blockers.push(blocker(
            operation,
            "ownership-state",
            ResourceBlockerCode::OwnershipUnreadable,
            issue.record,
            issue.description,
        ));
    }
    for claim in claims {
        let claim_matches = match operation {
            ResourceOperation::DeleteCluster { cluster_ref } => {
                claim.cluster_ref.as_str() == cluster_ref
            }
            ResourceOperation::DeleteAdapter { .. } | ResourceOperation::RebindAdapter { .. } => {
                false
            }
            _ => identity_matches(operation, &claim.target),
        };
        if claim_matches {
            let mut value = blocker(
                operation,
                "runtime-owner",
                ResourceBlockerCode::RuntimeResourceOwned,
                &claim.owner_id,
                "active cluster route generation owns this runtime target",
            );
            value.owner = Some(claim.server_ref);
            value.route = Some(claim.route.to_string());
            value.next_actions =
                vec!["stop and remove the owning server before retrying".to_string()];
            blockers.push(value);
        }
    }
    for generation in generations {
        if identity_matches(operation, &generation.identity) {
            let mut value = blocker(
                operation,
                "runtime-generation",
                ResourceBlockerCode::RuntimeResourceOwned,
                &generation.generation_id,
                "physical model runtime generation still owns this target",
            );
            value.next_actions = vec![
                "stop the runtime caller, then run tentgent runtime reconcile --apply".to_string(),
            ];
            blockers.push(value);
        }
    }
    Ok(blockers)
}

fn identity_matches(operation: &ResourceOperation, identity: &RuntimeExecutionIdentity) -> bool {
    match operation {
        ResourceOperation::DeleteModel { model_ref } => identity.model_ref() == Some(model_ref),
        ResourceOperation::RemoveModelCapability {
            model_ref,
            capability,
        } => {
            identity.model_ref() == Some(model_ref)
                && identity.capability()
                    == ModelRuntimeCapability::from_model_capability(*capability)
        }
        ResourceOperation::ReplaceModelCapabilities {
            model_ref,
            removed_capabilities,
        } => {
            identity.model_ref() == Some(model_ref)
                && removed_capabilities.iter().any(|capability| {
                    identity.capability()
                        == ModelRuntimeCapability::from_model_capability(*capability)
                })
        }
        ResourceOperation::DeleteAdapter {
            base_model_ref,
            capability: _,
            ..
        }
        | ResourceOperation::RebindAdapter {
            old_base_model_ref: base_model_ref,
            capability: _,
            ..
        } => base_model_ref
            .as_deref()
            .is_some_and(|model_ref| identity.model_ref() == Some(model_ref)),
        _ => false,
    }
}
