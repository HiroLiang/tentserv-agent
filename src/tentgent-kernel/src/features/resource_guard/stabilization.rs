use crate::{
    features::{
        resource_coordination::{
            stabilize_resource_transition, ResourceKey, ResourceTransitionAuthorization,
            StableResourceTransition,
        },
        resource_guard::{
            ResourceGuardRejection, ResourceGuardUseCase, ResourceMutationAuthorization,
            ResourceMutationPermit, ResourceOperation,
        },
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

pub struct StableMutationSnapshot<Token, State> {
    pub token: Token,
    pub operation: ResourceOperation,
    pub state: State,
}

pub enum StableMutationAuthorization<State> {
    Permitted {
        state: State,
        permit: ResourceMutationPermit,
    },
    Rejected(ResourceGuardRejection),
    Busy(crate::features::resource_coordination::ResourceBusy),
}

pub fn authorize_stable_resource_mutation<Token, State, Snapshot>(
    layout: &RuntimeLayout,
    guard: &dyn ResourceGuardUseCase,
    mut snapshot: Snapshot,
) -> KernelResult<StableMutationAuthorization<State>>
where
    Token: PartialEq,
    Snapshot: FnMut() -> KernelResult<StableMutationSnapshot<Token, State>>,
{
    let result = stabilize_resource_transition(
        "resource-mutation",
        || {
            let StableMutationSnapshot {
                token,
                operation,
                state,
            } = snapshot()?;
            Ok((
                (token, operation.clone()),
                MutationAttemptState { operation, state },
            ))
        },
        |snapshot| {
            Ok(match guard.authorize(layout, snapshot.operation.clone())? {
                ResourceMutationAuthorization::Permitted(permit) => {
                    ResourceTransitionAuthorization::Permitted(permit)
                }
                ResourceMutationAuthorization::Rejected(rejection) => {
                    ResourceTransitionAuthorization::Rejected(rejection)
                }
                ResourceMutationAuthorization::Busy(busy) => {
                    ResourceTransitionAuthorization::Busy(busy)
                }
            })
        },
        |snapshot| primary_key(&snapshot.operation),
    )?;
    Ok(match result {
        StableResourceTransition::Stable { state, permit } => {
            StableMutationAuthorization::Permitted {
                state: state.state,
                permit,
            }
        }
        StableResourceTransition::Rejected(rejection) => {
            StableMutationAuthorization::Rejected(rejection)
        }
        StableResourceTransition::Busy(busy) => StableMutationAuthorization::Busy(busy),
    })
}

struct MutationAttemptState<State> {
    operation: ResourceOperation,
    state: State,
}

fn primary_key(operation: &ResourceOperation) -> ResourceKey {
    use crate::features::resource_coordination::ResourceKind;

    let kind = match operation {
        ResourceOperation::DeleteModel { .. }
        | ResourceOperation::RemoveModelCapability { .. }
        | ResourceOperation::ReplaceModelCapabilities { .. } => ResourceKind::Model,
        ResourceOperation::DeleteAdapter { .. } | ResourceOperation::RebindAdapter { .. } => {
            ResourceKind::Adapter
        }
        ResourceOperation::DeleteDataset { .. } => ResourceKind::Dataset,
        ResourceOperation::DeleteTrainPlan { .. } => ResourceKind::TrainPlan,
        ResourceOperation::DeleteCluster { .. } | ResourceOperation::ReplaceCluster { .. } => {
            ResourceKind::Cluster
        }
        ResourceOperation::DeleteServerSpec { .. } => ResourceKind::Server,
    };
    ResourceKey::new(kind, operation.resource_ref())
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use crate::features::resource_coordination::ResourceCoordinationCode;

    use crate::{
        features::{model::domain::ModelCapability, resource_guard::StdResourceGuard},
        foundation::layout::{
            LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
        },
    };

    use super::*;

    #[test]
    fn capability_operation_change_retries_without_reusing_partial_locks() {
        let (layout, root) = test_layout("capability-operation-change");
        let mut reads = 0_u32;
        let result =
            authorize_stable_resource_mutation(&layout, &StdResourceGuard::default(), || {
                reads += 1;
                let removed_capabilities = if reads % 2 == 0 {
                    vec![ModelCapability::Chat, ModelCapability::Embedding]
                } else {
                    vec![ModelCapability::Chat]
                };
                Ok(StableMutationSnapshot {
                    token: removed_capabilities.clone(),
                    operation: ResourceOperation::ReplaceModelCapabilities {
                        model_ref: "model-a".to_string(),
                        removed_capabilities,
                    },
                    state: (),
                })
            })
            .expect("stabilize capability mutation");

        assert_unstable(result);
        assert_no_holder_metadata(&layout);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn adapter_rebind_key_change_retries_from_a_fresh_lock_set() {
        let (layout, root) = test_layout("adapter-rebind-change");
        let mut reads = 0_u32;
        let result =
            authorize_stable_resource_mutation(&layout, &StdResourceGuard::default(), || {
                reads += 1;
                let old_base_model_ref = Some(if reads % 2 == 0 {
                    "model-b".to_string()
                } else {
                    "model-a".to_string()
                });
                Ok(StableMutationSnapshot {
                    token: old_base_model_ref.clone(),
                    operation: ResourceOperation::RebindAdapter {
                        adapter_ref: "adapter-a".to_string(),
                        old_base_model_ref,
                        new_base_model_ref: Some("model-c".to_string()),
                        capability: Some(ModelCapability::Chat),
                    },
                    state: (),
                })
            })
            .expect("stabilize adapter mutation");

        assert_unstable(result);
        assert_no_holder_metadata(&layout);
        let _ = fs::remove_dir_all(root);
    }

    fn assert_unstable(result: StableMutationAuthorization<()>) {
        let StableMutationAuthorization::Busy(busy) = result else {
            panic!("expected unstable resource state");
        };
        assert_eq!(busy.code, ResourceCoordinationCode::ResourceStateUnstable);
        assert_eq!(busy.attempts, 3);
    }

    fn assert_no_holder_metadata(layout: &crate::foundation::layout::RuntimeLayout) {
        let holders = layout.locks_dir.join("resource-coordination/holders");
        if !holders.exists() {
            return;
        }
        let files = fs::read_dir(holders)
            .expect("holder root")
            .flat_map(|entry| fs::read_dir(entry.expect("holder key").path()).expect("holder key"))
            .count();
        assert_eq!(files, 0);
    }

    fn test_layout(label: &str) -> (crate::foundation::layout::RuntimeLayout, std::path::PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tentgent-resource-stabilization-{label}-{}-{nanos}",
            std::process::id()
        ));
        let layout = StdRuntimeLayoutResolver
            .resolve(RuntimeLayoutInput {
                mode: LayoutResolveMode::Create,
                home_dir: Some(root.join("home")),
                data_root_dir: Some(root.join("data")),
            })
            .expect("runtime layout");
        (layout, root)
    }
}
