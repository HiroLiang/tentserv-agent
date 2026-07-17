//! Model removal use case.

use std::sync::Arc;

use crate::features::model::domain::ModelRemovalOutcome;
use crate::features::model::ports::{
    ModelCatalogStore, ModelContentStore, ModelServerReferenceProbe, ModelSourceIndexStore,
};
use crate::features::resource_guard::{
    ResourceGuardUseCase, ResourceMutationAuthorization, ResourceMutationOutcome,
    ResourceOperation, StdResourceGuard,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::model_store_layout;
use super::port::{ModelRemoveRequest, ModelRemoveResult, ModelRemoveUseCase};

/// Standard model removal orchestration.
pub struct StdModelRemoveUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn ModelCatalogStore,
    source_indexes: &'a dyn ModelSourceIndexStore,
    content: &'a dyn ModelContentStore,
    guard: Arc<dyn ResourceGuardUseCase>,
}

impl<'a> StdModelRemoveUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ModelCatalogStore,
        source_indexes: &'a dyn ModelSourceIndexStore,
        content: &'a dyn ModelContentStore,
        _server_refs: &'a dyn ModelServerReferenceProbe,
    ) -> Self {
        Self::new_with_guard(
            layout_resolver,
            catalog,
            source_indexes,
            content,
            Arc::new(StdResourceGuard::default()),
        )
    }

    pub fn new_with_guard(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn ModelCatalogStore,
        source_indexes: &'a dyn ModelSourceIndexStore,
        content: &'a dyn ModelContentStore,
        guard: Arc<dyn ResourceGuardUseCase>,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            source_indexes,
            content,
            guard,
        }
    }

    pub fn remove_model_guarded(
        &self,
        request: ModelRemoveRequest,
    ) -> KernelResult<ResourceMutationOutcome<ModelRemoveResult>> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = model_store_layout(&layout);
        let inspection = self.catalog.inspect_model(&store, &request.selector)?;
        let model_ref = inspection.metadata.model_ref.clone();
        let authorization = self.guard.authorize(
            &layout,
            ResourceOperation::DeleteModel {
                model_ref: model_ref.to_string(),
            },
        )?;
        let _permit = match authorization {
            ResourceMutationAuthorization::Permitted(permit) => permit,
            ResourceMutationAuthorization::Rejected(rejection) => {
                return Ok(ResourceMutationOutcome::Blocked(rejection));
            }
            ResourceMutationAuthorization::Busy(busy) => {
                return Ok(ResourceMutationOutcome::Busy(busy));
            }
        };
        let inspection = self.catalog.inspect_model(&store, &request.selector)?;
        let removed_index_paths = self
            .source_indexes
            .remove_source_indexes(&store, &model_ref)?;
        self.content.remove_model_content(&store, &model_ref)?;
        Ok(ResourceMutationOutcome::Applied(ModelRemoveResult {
            layout,
            store,
            outcome: ModelRemovalOutcome {
                metadata: inspection.metadata,
                store_path: inspection.store_path,
                removed_index_paths,
            },
        }))
    }
}

impl ModelRemoveUseCase for StdModelRemoveUseCase<'_> {
    fn remove_model(&self, request: ModelRemoveRequest) -> KernelResult<ModelRemoveResult> {
        self.remove_model_guarded(request)?.into_compat_result()
    }
}
