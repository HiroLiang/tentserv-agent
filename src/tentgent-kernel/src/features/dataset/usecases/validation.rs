//! Dataset validation use case.

use crate::features::dataset::ports::{DatasetCatalogStore, DatasetValidator};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::dataset_store_layout;
use super::port::{
    DatasetValidateRequest, DatasetValidateResult, DatasetValidationTargetSelection,
    DatasetValidationUseCase,
};

/// Standard dataset validation orchestration.
pub struct StdDatasetValidationUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn DatasetCatalogStore,
    validator: &'a dyn DatasetValidator,
}

impl<'a> StdDatasetValidationUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn DatasetCatalogStore,
        validator: &'a dyn DatasetValidator,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
            validator,
        }
    }
}

impl DatasetValidationUseCase for StdDatasetValidationUseCase<'_> {
    fn validate_dataset(
        &self,
        request: DatasetValidateRequest,
    ) -> KernelResult<DatasetValidateResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        let (dataset, path) = match request.target {
            DatasetValidationTargetSelection::LocalPath(path) => (None, path),
            DatasetValidationTargetSelection::ManagedDataset(selector) => {
                let inspection = self.catalog.inspect_dataset(&store, &selector)?;
                let path = inspection.source_path.clone();
                (Some(inspection), path)
            }
        };
        let outcome = self.validator.validate_dataset_path(&path)?;

        Ok(DatasetValidateResult {
            layout,
            store,
            dataset,
            outcome,
        })
    }
}
