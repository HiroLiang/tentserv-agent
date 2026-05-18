use std::fs;

use crate::features::adapter::domain::AdapterStoreLayout;
use crate::features::adapter::ports::AdapterStoreLayoutInitializer;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Creates the standard adapter-store directory layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdAdapterStoreLayoutInitializer;

impl AdapterStoreLayoutInitializer for StdAdapterStoreLayoutInitializer {
    fn ensure_adapter_store_layout(&self, layout: &AdapterStoreLayout) -> KernelResult<()> {
        for path in [
            &layout.store_dir,
            &layout.by_base_dir,
            &layout.hf_index_dir,
            &layout.local_index_dir,
            &layout.train_run_index_dir,
            &layout.staging_dir,
        ] {
            fs::create_dir_all(path).map_err(|err| path_error("create directory", path, err))?;
        }

        Ok(())
    }
}
