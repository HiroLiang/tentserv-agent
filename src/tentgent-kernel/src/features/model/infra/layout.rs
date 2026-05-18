use std::fs;

use crate::features::model::domain::ModelStoreLayout;
use crate::features::model::ports::ModelStoreLayoutInitializer;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Creates the standard model-store directory layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdModelStoreLayoutInitializer;

impl ModelStoreLayoutInitializer for StdModelStoreLayoutInitializer {
    fn ensure_model_store_layout(&self, layout: &ModelStoreLayout) -> KernelResult<()> {
        for path in [
            &layout.store_dir,
            &layout.hf_index_dir,
            &layout.local_index_dir,
            &layout.staging_dir,
        ] {
            fs::create_dir_all(path).map_err(|err| path_error("create directory", path, err))?;
        }

        Ok(())
    }
}
