use std::fs;

use crate::features::dataset::domain::DatasetStoreLayout;
use crate::features::dataset::ports::DatasetStoreLayoutInitializer;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Creates the standard dataset-store directory layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDatasetStoreLayoutInitializer;

impl DatasetStoreLayoutInitializer for StdDatasetStoreLayoutInitializer {
    fn ensure_dataset_store_layout(&self, layout: &DatasetStoreLayout) -> KernelResult<()> {
        for path in [
            &layout.store_dir,
            &layout.local_index_dir,
            &layout.staging_dir,
        ] {
            fs::create_dir_all(path).map_err(|err| path_error("create directory", path, err))?;
        }

        Ok(())
    }
}
