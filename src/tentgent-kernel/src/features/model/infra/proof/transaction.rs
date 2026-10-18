//! One short, non-reentrant coordination boundary for every proof generation.

use crate::features::model::{
    domain::{
        ModelCapability, ModelMetadata, ModelRef, ModelStoreLayout,
        MODEL_CAPABILITY_CANONICAL_ORDER,
    },
    ports::ModelCatalogStore,
    proof_context::ModelProofContext,
};
use crate::features::resource_coordination::{
    infra::{coordination_root, FileResourceCoordinator},
    ResourceCoordinator, ResourceKey, ResourceKind, ResourceLockMode, ResourceLockRequest,
};
use crate::foundation::error::{KernelError, KernelResult};

use super::super::FileModelCatalogStore;

pub(super) fn transact<T>(
    context: &ModelProofContext<'_>,
    model_ref: &ModelRef,
    capability: Option<ModelCapability>,
    mode: ResourceLockMode,
    operation: impl FnOnce(&ModelStoreLayout, &ModelMetadata) -> KernelResult<T>,
) -> KernelResult<T> {
    let mut locks = vec![
        (ResourceKey::maintenance(), ResourceLockMode::Shared),
        (
            ResourceKey::new(ResourceKind::Model, model_ref.as_str()),
            ResourceLockMode::Shared,
        ),
    ];
    for current in MODEL_CAPABILITY_CANONICAL_ORDER {
        if capability.is_none_or(|selected| selected == current) {
            locks.push((
                ResourceKey::new(
                    ResourceKind::ModelCapability,
                    format!("{model_ref}|{current}"),
                ),
                mode,
            ));
        }
    }
    let _owned_permit = if let Some(permit) = context.permit() {
        if permit.coordination_root() != coordination_root(context.runtime())
            || !permit.covers_all(&locks)
        {
            return Err(KernelError::ResourceCoordinationUnavailable(
                "proof transaction requires a permit from the same runtime home covering every requested key and mode; release and reacquire the complete lock set".into(),
            ));
        }
        None
    } else {
        Some(
            FileResourceCoordinator
                .acquire(
                    context.runtime(),
                    ResourceLockRequest::new("model-proof-transaction", locks),
                )?
                .map_err(|busy| {
                    KernelError::ResourceCoordinationUnavailable(format!(
                        "{}; retry after {} ms",
                        busy.description, busy.retry_after_millis,
                    ))
                })?,
        )
    };

    let metadata = FileModelCatalogStore.load_model_metadata(context.store(), model_ref)?;
    if metadata.model_ref != *model_ref {
        return Err(KernelError::ModelStoreUnavailable(
            "proof model metadata does not match its canonical directory".into(),
        ));
    }
    if context
        .expected_metadata()
        .is_some_and(|expected| expected != &metadata)
    {
        return Err(KernelError::ResourceStateUnstable {
            resource: model_ref.to_string(),
            description: "model metadata changed before the proof transaction; resolve again"
                .into(),
            retry_after_millis: 250,
        });
    }
    operation(context.store(), &metadata)
}
