use std::fs;
use std::path::{Path, PathBuf};

use crate::features::dataset::domain::{DatasetRef, DatasetStoreLayout, LocalDatasetSourceIndex};
use crate::features::dataset::ports::DatasetSourceIndexStore;
use crate::foundation::error::KernelResult;

use super::error::{dataset_store_error, path_error};

/// Filesystem-backed source-index store for canonical datasets.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileDatasetSourceIndexStore;

impl DatasetSourceIndexStore for FileDatasetSourceIndexStore {
    fn save_local_source_index(
        &self,
        layout: &DatasetStoreLayout,
        index: &LocalDatasetSourceIndex,
    ) -> KernelResult<PathBuf> {
        fs::create_dir_all(&layout.local_index_dir).map_err(|err| {
            path_error(
                "create local dataset source-index directory",
                &layout.local_index_dir,
                err,
            )
        })?;
        let path = layout.local_index_path(&index.dataset_ref);
        write_toml(&path, index)?;
        Ok(path)
    }

    fn remove_source_indexes(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<Vec<PathBuf>> {
        let mut removed = Vec::new();

        let local_path = layout.local_index_path(dataset_ref);
        if local_path.exists() {
            fs::remove_file(&local_path)
                .map_err(|err| path_error("remove local dataset source index", &local_path, err))?;
            removed.push(local_path);
        }

        Ok(removed)
    }
}

fn write_toml<T: serde::Serialize>(path: &Path, value: &T) -> KernelResult<()> {
    let body = toml::to_string_pretty(value).map_err(|err| {
        dataset_store_error(format!("serialize dataset source index failed: {err}"))
    })?;
    fs::write(path, body).map_err(|err| path_error("write dataset source index", path, err))
}
