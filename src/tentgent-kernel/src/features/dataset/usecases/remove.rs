//! Dataset removal use case.

use crate::features::dataset::domain::DatasetRemovalOutcome;
use crate::features::dataset::ports::{
    DatasetCatalogStore, DatasetContentStore, DatasetReferenceGuard, DatasetSourceIndexStore,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{dataset_store_error, dataset_store_layout};
use super::port::{DatasetRemoveRequest, DatasetRemoveResult, DatasetRemoveUseCase};

/// Standard dataset removal orchestration.
pub struct StdDatasetRemoveUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn DatasetCatalogStore,
    source_indexes: &'a dyn DatasetSourceIndexStore,
    content: &'a dyn DatasetContentStore,
    reference_guard: &'a dyn DatasetReferenceGuard,
}

impl<'a> StdDatasetRemoveUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn DatasetCatalogStore,
        source_indexes: &'a dyn DatasetSourceIndexStore,
        content: &'a dyn DatasetContentStore,
        reference_guard: &'a dyn DatasetReferenceGuard,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            source_indexes,
            content,
            reference_guard,
        }
    }
}

impl DatasetRemoveUseCase for StdDatasetRemoveUseCase<'_> {
    fn remove_dataset(&self, request: DatasetRemoveRequest) -> KernelResult<DatasetRemoveResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        let inspection = self.catalog.inspect_dataset(&store, &request.selector)?;
        let dataset_ref = inspection.metadata.dataset_ref.clone();
        let blockers = self
            .reference_guard
            .train_refs_for_dataset(&layout, &dataset_ref)?;
        if !blockers.is_empty() {
            return Err(dataset_store_error(format!(
                "dataset `{}` is still referenced by train plan/run(s): {}",
                dataset_ref,
                blockers.join(", ")
            )));
        }

        let removed_index_paths = self
            .source_indexes
            .remove_source_indexes(&store, &dataset_ref)?;
        self.content.remove_dataset_content(&store, &dataset_ref)?;

        Ok(DatasetRemoveResult {
            layout,
            store,
            outcome: DatasetRemovalOutcome {
                metadata: inspection.metadata,
                store_path: inspection.store_path,
                removed_index_paths,
                blockers,
            },
        })
    }
}
