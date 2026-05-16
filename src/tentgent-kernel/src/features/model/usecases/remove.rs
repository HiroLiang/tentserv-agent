//! Model removal use case.

use crate::features::model::domain::ModelRemovalOutcome;
use crate::features::model::ports::{
    ModelCatalogStore, ModelContentStore, ModelServerReferenceProbe, ModelSourceIndexStore,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::model_store_layout;
use super::port::{ModelRemoveRequest, ModelRemoveResult, ModelRemoveUseCase};

/// Standard model removal orchestration.
pub struct StdModelRemoveUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn ModelCatalogStore,
    source_indexes: &'a dyn ModelSourceIndexStore,
    content: &'a dyn ModelContentStore,
    server_refs: &'a dyn ModelServerReferenceProbe,
}

impl<'a> StdModelRemoveUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ModelCatalogStore,
        source_indexes: &'a dyn ModelSourceIndexStore,
        content: &'a dyn ModelContentStore,
        server_refs: &'a dyn ModelServerReferenceProbe,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            source_indexes,
            content,
            server_refs,
        }
    }
}

impl ModelRemoveUseCase for StdModelRemoveUseCase<'_> {
    fn remove_model(&self, request: ModelRemoveRequest) -> KernelResult<ModelRemoveResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let inspection = self.catalog.inspect_model(&store, &request.selector)?;
        let model_ref = inspection.metadata.model_ref.clone();
        let blockers = self
            .server_refs
            .server_refs_for_model(&layout, &model_ref)?;
        if !blockers.is_empty() {
            return Err(KernelError::ModelStoreUnavailable(format!(
                "model `{}` is still referenced by server spec(s): {}",
                model_ref,
                blockers.join(", ")
            )));
        }

        let removed_index_paths = self
            .source_indexes
            .remove_source_indexes(&store, &model_ref)?;
        self.content.remove_model_content(&store, &model_ref)?;

        Ok(ModelRemoveResult {
            layout,
            store,
            outcome: ModelRemovalOutcome {
                metadata: inspection.metadata,
                store_path: inspection.store_path,
                removed_index_paths,
            },
        })
    }
}
