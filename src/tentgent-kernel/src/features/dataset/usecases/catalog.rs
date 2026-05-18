//! Dataset catalog read use case.

use crate::features::dataset::ports::DatasetCatalogStore;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::dataset_store_layout;
use super::port::{
    DatasetCatalogReadUseCase, DatasetInspectRequest, DatasetInspectResult, DatasetListRequest,
    DatasetListResult,
};

/// Standard dataset catalog read orchestration.
pub struct StdDatasetCatalogReadUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn DatasetCatalogStore,
}

impl<'a> StdDatasetCatalogReadUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn DatasetCatalogStore,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
        }
    }
}

impl DatasetCatalogReadUseCase for StdDatasetCatalogReadUseCase<'_> {
    fn list_datasets(&self, request: DatasetListRequest) -> KernelResult<DatasetListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        let datasets = self.catalog.list_datasets(&store)?;

        Ok(DatasetListResult {
            layout,
            store,
            datasets,
        })
    }

    fn inspect_dataset(
        &self,
        request: DatasetInspectRequest,
    ) -> KernelResult<DatasetInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        let dataset = self.catalog.inspect_dataset(&store, &request.selector)?;

        Ok(DatasetInspectResult {
            layout,
            store,
            dataset,
        })
    }
}
