//! Dataset export use case.

use crate::features::dataset::domain::DatasetExportOutcome;
use crate::features::dataset::ports::{DatasetCatalogStore, DatasetContentStore};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::dataset_store_layout;
use super::port::{DatasetExportRequest, DatasetExportResult, DatasetExportUseCase};

/// Standard dataset export orchestration.
pub struct StdDatasetExportUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn DatasetCatalogStore,
    content: &'a dyn DatasetContentStore,
}

impl<'a> StdDatasetExportUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn DatasetCatalogStore,
        content: &'a dyn DatasetContentStore,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            content,
        }
    }
}

impl DatasetExportUseCase for StdDatasetExportUseCase<'_> {
    fn export_dataset(&self, request: DatasetExportRequest) -> KernelResult<DatasetExportResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        let inspection = self.catalog.inspect_dataset(&store, &request.selector)?;
        let destination_path = self.content.export_source(
            &store,
            &inspection.metadata.dataset_ref,
            &request.destination_path,
        )?;

        Ok(DatasetExportResult {
            layout,
            store,
            outcome: DatasetExportOutcome {
                metadata: inspection.metadata,
                managed_source_path: inspection.source_path,
                destination_path,
            },
        })
    }
}
