use std::fs;

use crate::features::train::domain::TrainStoreLayout;
use crate::features::train::ports::TrainStoreLayoutInitializer;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Creates the standard LoRA training-store directory layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdTrainStoreLayoutInitializer;

impl TrainStoreLayoutInitializer for StdTrainStoreLayoutInitializer {
    fn ensure_train_store_layout(&self, layout: &TrainStoreLayout) -> KernelResult<()> {
        for path in [&layout.plans_dir, &layout.staging_dir] {
            fs::create_dir_all(path).map_err(|err| path_error("create directory", path, err))?;
        }

        Ok(())
    }
}
