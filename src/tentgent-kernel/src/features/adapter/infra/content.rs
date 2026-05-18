use std::fs;
use std::path::PathBuf;

use crate::features::adapter::domain::{AdapterRef, AdapterStoreLayout};
use crate::features::adapter::ports::{AdapterContentStore, StagedAdapterSource};
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Filesystem-backed canonical adapter content store.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileAdapterContentStore;

impl AdapterContentStore for FileAdapterContentStore {
    fn adapter_content_exists(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<bool> {
        Ok(layout.adapter_dir(adapter_ref).exists())
    }

    fn install_staged_source(
        &self,
        layout: &AdapterStoreLayout,
        staged: &StagedAdapterSource,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<PathBuf> {
        let adapter_dir = layout.adapter_dir(adapter_ref);
        fs::create_dir_all(&adapter_dir)
            .map_err(|err| path_error("create adapter directory", &adapter_dir, err))?;

        let destination = layout.source_dir(adapter_ref);
        fs::rename(&staged.source_dir, &destination).map_err(|err| {
            path_error(
                "move staged adapter source into store",
                &staged.source_dir,
                err,
            )
        })?;
        Ok(destination)
    }

    fn remove_adapter_content(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<()> {
        let path = layout.adapter_dir(adapter_ref);
        if path.exists() {
            fs::remove_dir_all(&path)
                .map_err(|err| path_error("remove canonical adapter directory", &path, err))?;
        }
        Ok(())
    }
}
