use std::fs;
use std::path::{Path, PathBuf};

use crate::features::adapter::domain::{AdapterRef, AdapterStoreLayout, BaseModelAdapterIndex};
use crate::features::adapter::ports::AdapterBaseIndexStore;
use crate::foundation::error::KernelResult;

use super::error::{adapter_store_error, path_error};

/// Filesystem-backed base-model index store for canonical adapters.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileAdapterBaseIndexStore;

impl AdapterBaseIndexStore for FileAdapterBaseIndexStore {
    fn save_base_model_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &BaseModelAdapterIndex,
    ) -> KernelResult<PathBuf> {
        let index_dir = layout.base_index_dir(&index.base_model_ref);
        fs::create_dir_all(&index_dir)
            .map_err(|err| path_error("create adapter base-index directory", &index_dir, err))?;
        let path = layout.base_index_path(&index.base_model_ref, &index.adapter_ref);
        write_toml(&path, index)?;
        Ok(path)
    }

    fn remove_base_model_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &BaseModelAdapterIndex,
    ) -> KernelResult<Option<PathBuf>> {
        let path = layout.base_index_path(&index.base_model_ref, &index.adapter_ref);
        if !path.exists() {
            return Ok(None);
        }

        fs::remove_file(&path)
            .map_err(|err| path_error("remove adapter base index", &path, err))?;
        if let Some(parent) = path.parent() {
            remove_dir_if_empty(parent)?;
        }
        Ok(Some(path))
    }

    fn remove_base_model_indexes(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<PathBuf>> {
        let mut removed = remove_matching_base_indexes(&layout.by_base_dir, adapter_ref)?;
        removed.sort();
        Ok(removed)
    }
}

fn write_toml<T: serde::Serialize>(path: &Path, value: &T) -> KernelResult<()> {
    let body = toml::to_string_pretty(value).map_err(|err| {
        adapter_store_error(format!("serialize adapter base index failed: {err}"))
    })?;
    fs::write(path, body).map_err(|err| path_error("write adapter base index", path, err))
}

fn remove_matching_base_indexes(
    root: &Path,
    adapter_ref: &AdapterRef,
) -> KernelResult<Vec<PathBuf>> {
    let mut removed = Vec::new();
    if !root.exists() {
        return Ok(removed);
    }

    for entry in fs::read_dir(root)
        .map_err(|err| path_error("read adapter base-index directory", root, err))?
    {
        let entry = entry.map_err(|err| {
            adapter_store_error(format!(
                "read entry in adapter base-index directory `{}` failed: {err}",
                root.display()
            ))
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| path_error("read adapter base-index entry type", &path, err))?;

        if file_type.is_dir() {
            removed.extend(remove_matching_base_indexes(&path, adapter_ref)?);
            remove_dir_if_empty(&path)?;
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }

        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read adapter base index", &path, err))?;
        let index: BaseModelAdapterIndex = toml::from_str(&body).map_err(|err| {
            adapter_store_error(format!(
                "parse adapter base index `{}` failed: {err}",
                path.display()
            ))
        })?;

        if index.adapter_ref == *adapter_ref {
            fs::remove_file(&path)
                .map_err(|err| path_error("remove adapter base index", &path, err))?;
            removed.push(path);
        }
    }

    Ok(removed)
}

fn remove_dir_if_empty(path: &Path) -> KernelResult<()> {
    if fs::read_dir(path)
        .map_err(|err| path_error("read adapter base-index directory", path, err))?
        .next()
        .is_none()
    {
        fs::remove_dir(path)
            .map_err(|err| path_error("remove empty adapter base-index directory", path, err))?;
    }

    Ok(())
}
