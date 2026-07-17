//! Dataset removal use case.

use std::sync::Arc;

use crate::features::dataset::domain::DatasetRemovalOutcome;
use crate::features::dataset::ports::{
    DatasetCatalogStore, DatasetContentStore, DatasetReferenceGuard, DatasetSourceIndexStore,
};
use crate::features::resource_guard::{
    ResourceGuardUseCase, ResourceMutationAuthorization, ResourceMutationOutcome,
    ResourceOperation, StdResourceGuard,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::dataset_store_layout;
use super::port::{DatasetRemoveRequest, DatasetRemoveResult, DatasetRemoveUseCase};

/// Standard dataset removal orchestration.
pub struct StdDatasetRemoveUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn DatasetCatalogStore,
    source_indexes: &'a dyn DatasetSourceIndexStore,
    content: &'a dyn DatasetContentStore,
    guard: Arc<dyn ResourceGuardUseCase>,
}

impl<'a> StdDatasetRemoveUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn DatasetCatalogStore,
        source_indexes: &'a dyn DatasetSourceIndexStore,
        content: &'a dyn DatasetContentStore,
        _reference_guard: &'a dyn DatasetReferenceGuard,
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
        catalog: &'a dyn DatasetCatalogStore,
        source_indexes: &'a dyn DatasetSourceIndexStore,
        content: &'a dyn DatasetContentStore,
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

    pub fn remove_dataset_guarded(
        &self,
        request: DatasetRemoveRequest,
    ) -> KernelResult<ResourceMutationOutcome<DatasetRemoveResult>> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        let inspection = self.catalog.inspect_dataset(&store, &request.selector)?;
        let dataset_ref = inspection.metadata.dataset_ref.clone();
        let authorization = self.guard.authorize(
            &layout,
            ResourceOperation::DeleteDataset {
                dataset_ref: dataset_ref.to_string(),
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
        let inspection = self.catalog.inspect_dataset(&store, &request.selector)?;
        let removed_index_paths = self
            .source_indexes
            .remove_source_indexes(&store, &dataset_ref)?;
        self.content.remove_dataset_content(&store, &dataset_ref)?;
        Ok(ResourceMutationOutcome::Applied(DatasetRemoveResult {
            layout,
            store,
            outcome: DatasetRemovalOutcome {
                metadata: inspection.metadata,
                store_path: inspection.store_path,
                removed_index_paths,
                blockers: Vec::new(),
            },
        }))
    }
}

impl DatasetRemoveUseCase for StdDatasetRemoveUseCase<'_> {
    fn remove_dataset(&self, request: DatasetRemoveRequest) -> KernelResult<DatasetRemoveResult> {
        self.remove_dataset_guarded(request)?.into_compat_result()
    }
}
