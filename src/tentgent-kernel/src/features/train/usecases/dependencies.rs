use crate::{
    features::{
        adapter::{
            domain::{
                validate_adapter_compatibility, AdapterBackendSupport, AdapterCompatibilityTarget,
                AdapterInspection, AdapterRefSelector, AdapterStoreLayout,
            },
            ports::AdapterCatalogStore,
        },
        dataset::{domain::DatasetInspection, ports::DatasetCatalogStore},
        model::{
            domain::{ModelCapability, ModelInspection, ModelRefSelector},
            ports::ModelCatalogStore,
        },
        resource_coordination::{ResourceKey, ResourceKind, ResourceLockMode},
        train::domain::{LoraTrainBackend, LoraTrainPlan},
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::RuntimeLayout,
    },
};

use super::common::{dataset_store_layout, model_store_layout};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TrainDependencySnapshot {
    pub model: ModelInspection,
    pub dataset: DatasetInspection,
    pub resume_adapter: Option<AdapterInspection>,
}

pub(super) fn inspect_train_dependencies(
    layout: &RuntimeLayout,
    plan: &LoraTrainPlan,
    model_catalog: &dyn ModelCatalogStore,
    dataset_catalog: &dyn DatasetCatalogStore,
    adapter_catalog: &dyn AdapterCatalogStore,
) -> KernelResult<TrainDependencySnapshot> {
    let model = model_catalog.inspect_model(
        &model_store_layout(layout),
        &ModelRefSelector::parse(&plan.model_ref).map_err(train_error)?,
    )?;
    if !model.metadata.supports_capability(ModelCapability::Chat) {
        return Err(train_error(format!(
            "model `{}` no longer advertises chat capability",
            plan.model_ref
        )));
    }
    let dataset = dataset_catalog.inspect_dataset(
        &dataset_store_layout(layout),
        &crate::features::dataset::domain::DatasetRefSelector::parse(&plan.dataset_ref)
            .map_err(train_error)?,
    )?;
    let resume_adapter_ref = plan
        .backend_config
        .mlx
        .as_ref()
        .and_then(|config| config.resume_adapter_ref.as_deref());
    let resume_adapter = match resume_adapter_ref {
        Some(adapter_ref) => {
            let adapter = adapter_catalog.inspect_adapter(
                &AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone()),
                &AdapterRefSelector::parse(adapter_ref).map_err(train_error)?,
            )?;
            let backend = match plan.backend {
                Some(LoraTrainBackend::Mlx) => AdapterBackendSupport::Mlx,
                Some(LoraTrainBackend::Peft) => AdapterBackendSupport::TransformersPeft,
                None => {
                    return Err(train_error(
                        "resume adapter compatibility cannot be verified for a blocked plan",
                    ))
                }
            };
            validate_adapter_compatibility(
                &adapter.metadata,
                &AdapterCompatibilityTarget {
                    base_model_ref: model.metadata.model_ref.clone(),
                    base_model_source_repo: model.metadata.source_repo.clone(),
                    base_model_source_revision: model.metadata.source_revision.clone(),
                    base_model_capabilities: model.metadata.model_capabilities.clone(),
                    required_capability: ModelCapability::Chat,
                    backend,
                },
            )
            .map_err(|error| {
                train_error(format!(
                    "resume adapter `{adapter_ref}` is incompatible with plan model/backend: {error}"
                ))
            })?;
            Some(adapter)
        }
        None => None,
    };
    Ok(TrainDependencySnapshot {
        model,
        dataset,
        resume_adapter,
    })
}

pub(super) fn train_dependency_locks(
    plan: &LoraTrainPlan,
    plan_mode: ResourceLockMode,
) -> Vec<(ResourceKey, ResourceLockMode)> {
    let mut locks = vec![
        (ResourceKey::maintenance(), ResourceLockMode::Shared),
        (
            ResourceKey::new(ResourceKind::TrainPlan, &plan.plan_ref),
            plan_mode,
        ),
        (
            ResourceKey::new(ResourceKind::Model, &plan.model_ref),
            ResourceLockMode::Shared,
        ),
        (
            ResourceKey::new(
                ResourceKind::ModelCapability,
                format!("{}|{}", plan.model_ref, ModelCapability::Chat),
            ),
            ResourceLockMode::Shared,
        ),
        (
            ResourceKey::new(ResourceKind::Dataset, &plan.dataset_ref),
            ResourceLockMode::Shared,
        ),
    ];
    if let Some(adapter_ref) = plan
        .backend_config
        .mlx
        .as_ref()
        .and_then(|config| config.resume_adapter_ref.as_deref())
    {
        locks.push((
            ResourceKey::new(ResourceKind::Adapter, adapter_ref),
            ResourceLockMode::Shared,
        ));
    }
    locks
}

pub(super) fn busy_error(
    busy: crate::features::resource_coordination::ResourceBusy,
) -> KernelError {
    if busy.code
        == crate::features::resource_coordination::ResourceCoordinationCode::ResourceStateUnstable
    {
        return KernelError::ResourceStateUnstable {
            resource: busy.key.label(),
            description: busy.description,
            retry_after_millis: busy.retry_after_millis,
        };
    }
    KernelError::ResourceCoordinationUnavailable(format!(
        "{}; retry after {} ms",
        busy.description, busy.retry_after_millis
    ))
}

fn train_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::TrainStoreUnavailable(error.to_string())
}
