use std::fs;
use std::path::{Path, PathBuf};

use crate::features::dataset::domain::DatasetRef;
use crate::features::dataset::domain::DatasetStoreLayout;
use crate::features::dataset::ports::{DatasetContentStore, StagedDatasetSource};
use crate::foundation::error::KernelResult;

use super::error::{dataset_store_error, path_error};
use super::staging::copy_dir_contents;

/// Filesystem-backed canonical dataset content store.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileDatasetContentStore;

impl DatasetContentStore for FileDatasetContentStore {
    fn dataset_content_exists(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<bool> {
        Ok(layout.dataset_dir(dataset_ref).exists())
    }

    fn install_staged_source(
        &self,
        layout: &DatasetStoreLayout,
        staged: &StagedDatasetSource,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<PathBuf> {
        let dataset_dir = layout.dataset_dir(dataset_ref);
        fs::create_dir_all(&dataset_dir)
            .map_err(|err| path_error("create dataset directory", &dataset_dir, err))?;

        let destination = layout.source_dir(dataset_ref);
        if destination.exists() {
            return Err(dataset_store_error(format!(
                "canonical dataset source already exists: `{}`",
                destination.display()
            )));
        }
        fs::rename(&staged.source_dir, &destination).map_err(|err| {
            path_error(
                "move staged dataset source into store",
                &staged.source_dir,
                err,
            )
        })?;
        Ok(destination)
    }

    fn export_source(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
        destination: &Path,
    ) -> KernelResult<PathBuf> {
        ensure_export_destination(destination)?;
        let managed_source = layout.source_dir(dataset_ref);
        fs::create_dir_all(destination)
            .map_err(|err| path_error("create dataset export directory", destination, err))?;
        copy_dir_contents(&managed_source, destination)?;
        Ok(destination.to_path_buf())
    }

    fn remove_dataset_content(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<()> {
        let path = layout.dataset_dir(dataset_ref);
        if path.exists() {
            fs::remove_dir_all(&path)
                .map_err(|err| path_error("remove canonical dataset directory", &path, err))?;
        }
        Ok(())
    }
}

fn ensure_export_destination(destination: &Path) -> KernelResult<()> {
    if !destination.exists() {
        return Ok(());
    }

    if !destination.is_dir() {
        return Err(dataset_store_error(format!(
            "export destination exists but is not a directory: `{}`",
            destination.display()
        )));
    }

    if fs::read_dir(destination)
        .map_err(|err| path_error("read dataset export destination", destination, err))?
        .next()
        .is_some()
    {
        return Err(dataset_store_error(format!(
            "export destination already exists and is not empty: `{}`",
            destination.display()
        )));
    }

    Ok(())
}
