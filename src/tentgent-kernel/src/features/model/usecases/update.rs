//! Model capability metadata update use case.

use std::sync::Arc;

use crate::features::model::domain::{
    infer_mlx_runtime_family, normalize_model_capabilities, ModelCapability, ModelCapabilitySource,
};
use crate::features::model::ports::{ModelCatalogStore, ModelServerReferenceProbe};
use crate::features::resource_guard::{
    authorize_stable_resource_mutation, ResourceGuardUseCase, ResourceMutationOutcome,
    ResourceOperation, StableMutationAuthorization, StableMutationSnapshot, StdResourceGuard,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::model_store_layout;
use super::port::{
    ModelCapabilityMutation, ModelCapabilityUpdateRequest, ModelCapabilityUpdateResult,
    ModelCapabilityUpdateUseCase,
};

/// Standard model capability metadata correction orchestration.
pub struct StdModelCapabilityUpdateUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn ModelCatalogStore,
    guard: Arc<dyn ResourceGuardUseCase>,
}

impl<'a> StdModelCapabilityUpdateUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ModelCatalogStore,
        _refs: &'a dyn ModelServerReferenceProbe,
    ) -> Self {
        Self::new_with_guard(
            layout_resolver,
            catalog,
            Arc::new(StdResourceGuard::default()),
        )
    }

    pub fn new_with_guard(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ModelCatalogStore,
        guard: Arc<dyn ResourceGuardUseCase>,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            guard,
        }
    }

    pub fn update_model_capability_guarded(
        &self,
        request: ModelCapabilityUpdateRequest,
    ) -> KernelResult<ResourceMutationOutcome<ModelCapabilityUpdateResult>> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let mutation = request.mutation;
        let authorization =
            authorize_stable_resource_mutation(&layout, self.guard.as_ref(), || {
                let inspection = self.catalog.inspect_model(&store, &request.selector)?;
                let previous_capabilities = normalize_model_capabilities(
                    inspection.metadata.model_capabilities.iter().copied(),
                );
                let next_capabilities =
                    apply_capability_mutation(&previous_capabilities, mutation.clone())?;
                let removed_capabilities =
                    capabilities_difference(&previous_capabilities, &next_capabilities);
                let token = ModelCapabilityDependencyToken {
                    metadata: inspection.metadata.clone(),
                    next_capabilities: next_capabilities.clone(),
                };
                Ok(StableMutationSnapshot {
                    token,
                    operation: ResourceOperation::ReplaceModelCapabilities {
                        model_ref: inspection.metadata.model_ref.to_string(),
                        removed_capabilities,
                    },
                    state: ModelCapabilityMutationState {
                        inspection,
                        previous_capabilities,
                        next_capabilities,
                    },
                })
            })?;
        let (state, _permit) = match authorization {
            StableMutationAuthorization::Permitted { state, permit } => (state, permit),
            StableMutationAuthorization::Rejected(rejection) => {
                return Ok(ResourceMutationOutcome::Blocked(rejection));
            }
            StableMutationAuthorization::Busy(busy) => {
                return Ok(ResourceMutationOutcome::Busy(busy));
            }
        };
        let mut inspection = state.inspection;
        let previous_capabilities = state.previous_capabilities;
        let next_capabilities = state.next_capabilities;
        let added_capabilities =
            capabilities_difference(&next_capabilities, &previous_capabilities);
        let removed_capabilities =
            capabilities_difference(&previous_capabilities, &next_capabilities);
        inspection.metadata.model_capabilities = next_capabilities;
        inspection.metadata.model_capability_source = ModelCapabilitySource::ManualUpdate;
        inspection.metadata.mlx_runtime_family = infer_mlx_runtime_family(
            inspection.metadata.primary_format,
            &inspection.metadata.model_capabilities,
        );
        self.catalog
            .save_model_metadata(&store, &inspection.metadata)?;
        Ok(ResourceMutationOutcome::Applied(
            ModelCapabilityUpdateResult {
                layout,
                store,
                model: inspection,
                previous_capabilities,
                added_capabilities,
                removed_capabilities,
            },
        ))
    }
}

#[derive(PartialEq, Eq)]
struct ModelCapabilityDependencyToken {
    metadata: crate::features::model::domain::ModelMetadata,
    next_capabilities: Vec<ModelCapability>,
}

struct ModelCapabilityMutationState {
    inspection: crate::features::model::domain::ModelInspection,
    previous_capabilities: Vec<ModelCapability>,
    next_capabilities: Vec<ModelCapability>,
}

impl ModelCapabilityUpdateUseCase for StdModelCapabilityUpdateUseCase<'_> {
    fn update_model_capability(
        &self,
        request: ModelCapabilityUpdateRequest,
    ) -> KernelResult<ModelCapabilityUpdateResult> {
        self.update_model_capability_guarded(request)?
            .into_compat_result()
    }
}

fn apply_capability_mutation(
    previous: &[ModelCapability],
    mutation: ModelCapabilityMutation,
) -> KernelResult<Vec<ModelCapability>> {
    let next = match mutation {
        ModelCapabilityMutation::Set(capabilities) => normalize_model_capabilities(capabilities),
        ModelCapabilityMutation::AddRemove { add, remove } => {
            let mut next = previous.to_vec();
            next.extend(add);
            next.retain(|capability| !remove.contains(capability));
            normalize_model_capabilities(next)
        }
    };

    if next.is_empty() {
        return Err(KernelError::UnsupportedTarget(
            "model capability set must not be empty".to_string(),
        ));
    }

    Ok(next)
}

fn capabilities_difference(
    left: &[ModelCapability],
    right: &[ModelCapability],
) -> Vec<ModelCapability> {
    left.iter()
        .copied()
        .filter(|capability| !right.contains(capability))
        .collect()
}
