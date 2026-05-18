//! Dataset diff use case.

use crate::features::dataset::ports::{DatasetDiffTarget, DatasetDiffer};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::dataset_store_layout;
use super::port::{
    DatasetDiffRequest, DatasetDiffResult, DatasetDiffRightSelection, DatasetDiffUseCase,
};

/// Standard dataset diff orchestration.
pub struct StdDatasetDiffUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    differ: &'a dyn DatasetDiffer,
}

impl<'a> StdDatasetDiffUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        differ: &'a dyn DatasetDiffer,
    ) -> Self {
        Self {
            layout_resolver,
            differ,
        }
    }
}

impl DatasetDiffUseCase for StdDatasetDiffUseCase<'_> {
    fn diff_dataset(&self, request: DatasetDiffRequest) -> KernelResult<DatasetDiffResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = dataset_store_layout(&layout);
        let right = match request.right {
            DatasetDiffRightSelection::ManagedDataset(selector) => {
                DatasetDiffTarget::Dataset(selector)
            }
            DatasetDiffRightSelection::LocalPath(path) => DatasetDiffTarget::LocalPath(path),
        };
        let outcome = self.differ.diff_dataset(&store, &request.left, right)?;

        Ok(DatasetDiffResult {
            layout,
            store,
            outcome,
        })
    }
}
