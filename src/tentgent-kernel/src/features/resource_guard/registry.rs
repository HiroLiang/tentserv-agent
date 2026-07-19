use std::{fs, sync::Arc};

use crate::{
    features::{
        cluster::domain::{ClusterDefinition, ClusterRouteTarget},
        resource_coordination::{
            infra::FileResourceCoordinator, ResourceCoordinator, ResourceKey, ResourceKind,
            ResourceLockMode, ResourceLockRequest,
        },
        resource_guard::{
            ports::ResourceGuardUseCase,
            validators::{
                validate_delete_adapter, validate_delete_cluster, validate_delete_dataset,
                validate_delete_model, validate_delete_server_spec, validate_delete_train_plan,
                validate_rebind_adapter, validate_remove_model_capability,
                validate_replace_cluster, validate_replace_model_capabilities,
            },
            ResourceBlocker, ResourceGuardCode, ResourceGuardRejection,
            ResourceMutationAuthorization, ResourceMutationPermit, ResourceOperation,
        },
        runtime_ownership::{
            FileRuntimeOwnershipStore, OwnershipProcessProbe, RuntimeOwnershipStore,
            StdOwnershipProcessProbe,
        },
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

#[derive(Clone)]
pub struct ResourceGuardDependencies {
    pub coordinator: Arc<dyn ResourceCoordinator>,
    pub ownership_store: Arc<dyn RuntimeOwnershipStore>,
    pub process_probe: Arc<dyn OwnershipProcessProbe>,
}

impl Default for ResourceGuardDependencies {
    fn default() -> Self {
        Self {
            coordinator: Arc::new(FileResourceCoordinator),
            ownership_store: Arc::new(FileRuntimeOwnershipStore),
            process_probe: Arc::new(StdOwnershipProcessProbe),
        }
    }
}

#[derive(Clone)]
pub struct StdResourceGuard {
    dependencies: ResourceGuardDependencies,
}

impl StdResourceGuard {
    pub fn new_with_dependencies(dependencies: ResourceGuardDependencies) -> Self {
        Self { dependencies }
    }
}

impl Default for StdResourceGuard {
    fn default() -> Self {
        Self::new_with_dependencies(ResourceGuardDependencies::default())
    }
}

impl ResourceGuardUseCase for StdResourceGuard {
    fn authorize(
        &self,
        layout: &RuntimeLayout,
        operation: ResourceOperation,
    ) -> KernelResult<ResourceMutationAuthorization> {
        let lock_request =
            ResourceLockRequest::new(operation.code(), lock_set(layout, &operation)?);
        let coordination = match self
            .dependencies
            .coordinator
            .acquire(layout, lock_request)?
        {
            Ok(permit) => permit,
            Err(busy) => return Ok(ResourceMutationAuthorization::Busy(busy)),
        };
        let context = crate::features::resource_guard::probes::ResourceGuardProbeContext {
            ownership_store: self.dependencies.ownership_store.as_ref(),
            process_probe: self.dependencies.process_probe.as_ref(),
        };
        let mut blockers = validate_operation(&context, layout, &operation)?;
        blockers.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        blockers.dedup();
        if blockers.is_empty() {
            Ok(ResourceMutationAuthorization::Permitted(
                ResourceMutationPermit::new(operation, coordination),
            ))
        } else {
            let blocker_summary = blockers
                .iter()
                .map(|blocker| {
                    format!(
                        "{} {} ({})",
                        blocker.kind, blocker.reference, blocker.reason
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Ok(ResourceMutationAuthorization::Rejected(
                ResourceGuardRejection {
                    code: rejection_code(&operation),
                    operation: operation.code().to_string(),
                    resource: operation.resource_ref().to_string(),
                    description: format!(
                        "resource operation `{}` is blocked by {} active or stored reference(s): {blocker_summary}",
                        operation.code(),
                        blockers.len()
                    ),
                    blockers,
                },
            ))
        }
    }
}

fn rejection_code(operation: &ResourceOperation) -> ResourceGuardCode {
    match operation {
        ResourceOperation::DeleteModel { .. } => ResourceGuardCode::ModelInUse,
        ResourceOperation::RemoveModelCapability { .. }
        | ResourceOperation::ReplaceModelCapabilities { .. } => ResourceGuardCode::CapabilityInUse,
        ResourceOperation::DeleteAdapter { .. } | ResourceOperation::RebindAdapter { .. } => {
            ResourceGuardCode::AdapterInUse
        }
        ResourceOperation::DeleteDataset { .. } => ResourceGuardCode::DatasetInUse,
        ResourceOperation::DeleteTrainPlan { .. } => ResourceGuardCode::TrainPlanInUse,
        ResourceOperation::DeleteCluster { .. } | ResourceOperation::ReplaceCluster { .. } => {
            ResourceGuardCode::ClusterInUse
        }
        ResourceOperation::DeleteServerSpec { .. } => ResourceGuardCode::ServerInUse,
    }
}

fn lock_set(
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<(ResourceKey, ResourceLockMode)>> {
    let mut locks = vec![(ResourceKey::maintenance(), ResourceLockMode::Shared)];
    match operation {
        ResourceOperation::DeleteModel { model_ref } => locks.push((
            ResourceKey::new(ResourceKind::Model, model_ref),
            ResourceLockMode::Exclusive,
        )),
        ResourceOperation::RemoveModelCapability {
            model_ref,
            capability: _,
        } => {
            locks.push((
                ResourceKey::new(ResourceKind::Model, model_ref),
                ResourceLockMode::Exclusive,
            ));
        }
        ResourceOperation::ReplaceModelCapabilities {
            model_ref,
            removed_capabilities: _,
        } => {
            locks.push((
                ResourceKey::new(ResourceKind::Model, model_ref),
                ResourceLockMode::Exclusive,
            ));
        }
        ResourceOperation::DeleteAdapter {
            adapter_ref,
            base_model_ref,
            ..
        } => {
            locks.push((
                ResourceKey::new(ResourceKind::Adapter, adapter_ref),
                ResourceLockMode::Exclusive,
            ));
            if let Some(model_ref) = base_model_ref {
                locks.push((
                    ResourceKey::new(ResourceKind::Model, model_ref),
                    ResourceLockMode::Exclusive,
                ));
            }
        }
        ResourceOperation::RebindAdapter {
            adapter_ref,
            old_base_model_ref,
            new_base_model_ref,
            ..
        } => {
            locks.push((
                ResourceKey::new(ResourceKind::Adapter, adapter_ref),
                ResourceLockMode::Exclusive,
            ));
            if let Some(model_ref) = old_base_model_ref {
                locks.push((
                    ResourceKey::new(ResourceKind::Model, model_ref),
                    ResourceLockMode::Exclusive,
                ));
            }
            if let Some(model_ref) = new_base_model_ref {
                locks.push((
                    ResourceKey::new(ResourceKind::Model, model_ref),
                    ResourceLockMode::Shared,
                ));
            }
        }
        ResourceOperation::DeleteDataset { dataset_ref } => locks.push((
            ResourceKey::new(ResourceKind::Dataset, dataset_ref),
            ResourceLockMode::Exclusive,
        )),
        ResourceOperation::DeleteTrainPlan { plan_ref } => {
            locks.push((
                ResourceKey::new(ResourceKind::TrainPlan, plan_ref),
                ResourceLockMode::Exclusive,
            ));
            for run_ref in train_run_refs(layout, plan_ref)? {
                locks.push((
                    ResourceKey::new(ResourceKind::TrainRun, run_ref),
                    ResourceLockMode::Exclusive,
                ));
            }
        }
        ResourceOperation::DeleteCluster { cluster_ref }
        | ResourceOperation::ReplaceCluster { cluster_ref, .. } => {
            locks.push((
                ResourceKey::new(ResourceKind::Cluster, cluster_ref),
                ResourceLockMode::Exclusive,
            ));
            if let ResourceOperation::ReplaceCluster { definition, .. } = operation {
                add_cluster_target_locks(&mut locks, definition);
            }
        }
        ResourceOperation::DeleteServerSpec { server_ref } => locks.push((
            ResourceKey::new(ResourceKind::Server, server_ref),
            ResourceLockMode::Exclusive,
        )),
    }
    Ok(locks)
}

fn validate_operation(
    context: &crate::features::resource_guard::probes::ResourceGuardProbeContext<'_>,
    layout: &RuntimeLayout,
    operation: &ResourceOperation,
) -> KernelResult<Vec<ResourceBlocker>> {
    match operation {
        ResourceOperation::DeleteModel { .. } => validate_delete_model(context, layout, operation),
        ResourceOperation::RemoveModelCapability { .. } => {
            validate_remove_model_capability(context, layout, operation)
        }
        ResourceOperation::ReplaceModelCapabilities { .. } => {
            validate_replace_model_capabilities(context, layout, operation)
        }
        ResourceOperation::DeleteAdapter { .. } => {
            validate_delete_adapter(context, layout, operation)
        }
        ResourceOperation::RebindAdapter { .. } => {
            validate_rebind_adapter(context, layout, operation)
        }
        ResourceOperation::DeleteDataset { .. } => {
            validate_delete_dataset(context, layout, operation)
        }
        ResourceOperation::DeleteTrainPlan { .. } => {
            validate_delete_train_plan(context, layout, operation)
        }
        ResourceOperation::DeleteCluster { .. } => {
            validate_delete_cluster(context, layout, operation)
        }
        ResourceOperation::ReplaceCluster { .. } => {
            validate_replace_cluster(context, layout, operation)
        }
        ResourceOperation::DeleteServerSpec { .. } => {
            validate_delete_server_spec(context, layout, operation)
        }
    }
}

fn add_cluster_target_locks(
    locks: &mut Vec<(ResourceKey, ResourceLockMode)>,
    definition: &ClusterDefinition,
) {
    for (route, target) in &definition.routes {
        if let ClusterRouteTarget::LocalModel { model_ref, .. } = target {
            locks.push((
                ResourceKey::new(ResourceKind::Model, model_ref.as_str()),
                ResourceLockMode::Shared,
            ));
            locks.push((
                ResourceKey::new(
                    ResourceKind::ModelCapability,
                    format!("{}|{}", model_ref, route.model_capability()),
                ),
                ResourceLockMode::Shared,
            ));
        }
    }
}

fn train_run_refs(layout: &RuntimeLayout, plan_ref: &str) -> KernelResult<Vec<String>> {
    let runs = layout
        .train_dir
        .join("lora/plans")
        .join(plan_ref)
        .join("runs");
    let entries = match fs::read_dir(runs) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(guard_error(error)),
    };
    let mut refs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(guard_error)?;
        if entry.file_type().map_err(guard_error)?.is_dir() {
            refs.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(refs)
}

fn guard_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::ResourceCoordinationUnavailable(format!(
        "resource guard lock planning failed: {error}"
    ))
}
