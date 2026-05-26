//! Model capability metadata update use case.

use crate::features::model::domain::{
    infer_mlx_runtime_family, normalize_model_capabilities, ModelCapability, ModelCapabilitySource,
};
use crate::features::model::ports::ModelCatalogStore;
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
}

impl<'a> StdModelCapabilityUpdateUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ModelCatalogStore,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
        }
    }
}

impl ModelCapabilityUpdateUseCase for StdModelCapabilityUpdateUseCase<'_> {
    fn update_model_capability(
        &self,
        request: ModelCapabilityUpdateRequest,
    ) -> KernelResult<ModelCapabilityUpdateResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let mut inspection = self.catalog.inspect_model(&store, &request.selector)?;
        let previous_capabilities =
            normalize_model_capabilities(inspection.metadata.model_capabilities.iter().copied());
        let next_capabilities =
            apply_capability_mutation(&previous_capabilities, request.mutation)?;
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

        Ok(ModelCapabilityUpdateResult {
            layout,
            store,
            model: inspection,
            previous_capabilities,
            added_capabilities,
            removed_capabilities,
        })
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
