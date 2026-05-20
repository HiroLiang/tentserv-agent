//! Model capability metadata update use case.

use crate::features::model::domain::{infer_mlx_runtime_family, ModelCapabilitySource};
use crate::features::model::ports::ModelCatalogStore;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::model_store_layout;
use super::port::{
    ModelCapabilityUpdateRequest, ModelCapabilityUpdateResult, ModelCapabilityUpdateUseCase,
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

        inspection.metadata.model_capabilities = vec![request.capability];
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
        })
    }
}
