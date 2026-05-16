//! Model catalog read use case.

use crate::features::model::ports::ModelCatalogStore;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::model_store_layout;
use super::port::{
    ModelCatalogReadUseCase, ModelInspectRequest, ModelInspectResult, ModelListRequest,
    ModelListResult,
};

/// Standard model catalog read orchestration.
pub struct StdModelCatalogReadUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn ModelCatalogStore,
}

impl<'a> StdModelCatalogReadUseCase<'a> {
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

impl ModelCatalogReadUseCase for StdModelCatalogReadUseCase<'_> {
    fn list_models(&self, request: ModelListRequest) -> KernelResult<ModelListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let models = self.catalog.list_models(&store)?;

        Ok(ModelListResult {
            layout,
            store,
            models,
        })
    }

    fn inspect_model(&self, request: ModelInspectRequest) -> KernelResult<ModelInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let model = self.catalog.inspect_model(&store, &request.selector)?;

        Ok(ModelInspectResult {
            layout,
            store,
            model,
        })
    }
}
