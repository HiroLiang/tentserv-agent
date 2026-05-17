use std::fs;
use std::path::{Path, PathBuf};

use crate::features::adapter::domain::{
    AdapterRef, AdapterStoreLayout, HfAdapterSourceIndex, LocalAdapterSourceIndex,
    TrainRunAdapterSourceIndex,
};
use crate::features::adapter::ports::AdapterSourceIndexStore;
use crate::foundation::error::KernelResult;

use super::error::{adapter_store_error, path_error};

/// Filesystem-backed source-index store for canonical adapters.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileAdapterSourceIndexStore;

impl AdapterSourceIndexStore for FileAdapterSourceIndexStore {
    fn save_local_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &LocalAdapterSourceIndex,
    ) -> KernelResult<PathBuf> {
        fs::create_dir_all(&layout.local_index_dir).map_err(|err| {
            path_error(
                "create local adapter source-index directory",
                &layout.local_index_dir,
                err,
            )
        })?;
        let path = layout.local_index_path(&index.adapter_ref);
        write_toml(&path, index)?;
        Ok(path)
    }

    fn save_hf_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &HfAdapterSourceIndex,
    ) -> KernelResult<PathBuf> {
        let repo_dir = layout.hf_index_dir_for_repo(&index.source_repo);
        fs::create_dir_all(&repo_dir).map_err(|err| {
            path_error("create Hugging Face source-index directory", &repo_dir, err)
        })?;
        let path = layout.hf_index_path(&index.source_repo, &index.source_revision);
        write_toml(&path, index)?;
        Ok(path)
    }

    fn save_train_run_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &TrainRunAdapterSourceIndex,
    ) -> KernelResult<PathBuf> {
        fs::create_dir_all(&layout.train_run_index_dir).map_err(|err| {
            path_error(
                "create training-run adapter source-index directory",
                &layout.train_run_index_dir,
                err,
            )
        })?;
        let path = layout.train_run_index_path(&index.training_run_ref);
        write_toml(&path, index)?;
        Ok(path)
    }

    fn remove_source_indexes(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<PathBuf>> {
        let mut removed = Vec::new();

        let local_path = layout.local_index_path(adapter_ref);
        remove_file_if_exists(
            &mut removed,
            local_path,
            "remove local adapter source index",
        )?;

        removed.extend(remove_matching_hf_indexes(
            &layout.hf_index_dir,
            adapter_ref,
        )?);
        removed.extend(remove_matching_train_run_indexes(
            &layout.train_run_index_dir,
            adapter_ref,
        )?);
        removed.sort();

        Ok(removed)
    }
}

fn write_toml<T: serde::Serialize>(path: &Path, value: &T) -> KernelResult<()> {
    let body = toml::to_string_pretty(value).map_err(|err| {
        adapter_store_error(format!("serialize adapter source index failed: {err}"))
    })?;
    fs::write(path, body).map_err(|err| path_error("write adapter source index", path, err))
}

fn remove_file_if_exists(
    removed: &mut Vec<PathBuf>,
    path: PathBuf,
    action: &str,
) -> KernelResult<()> {
    if path.exists() {
        fs::remove_file(&path).map_err(|err| path_error(action, &path, err))?;
        removed.push(path);
    }

    Ok(())
}

fn remove_matching_hf_indexes(root: &Path, adapter_ref: &AdapterRef) -> KernelResult<Vec<PathBuf>> {
    let mut removed = Vec::new();
    if !root.exists() {
        return Ok(removed);
    }

    for entry in fs::read_dir(root)
        .map_err(|err| path_error("read Hugging Face source-index directory", root, err))?
    {
        let entry = entry.map_err(|err| {
            adapter_store_error(format!(
                "read entry in Hugging Face source-index directory `{}` failed: {err}",
                root.display()
            ))
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| path_error("read source-index entry type", &path, err))?;

        if file_type.is_dir() {
            removed.extend(remove_matching_hf_indexes(&path, adapter_ref)?);
            remove_dir_if_empty(&path)?;
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }

        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read HF source index", &path, err))?;
        let index: HfAdapterSourceIndex = toml::from_str(&body).map_err(|err| {
            adapter_store_error(format!(
                "parse HF source index `{}` failed: {err}",
                path.display()
            ))
        })?;

        if index.adapter_ref == *adapter_ref {
            fs::remove_file(&path)
                .map_err(|err| path_error("remove HF adapter source index", &path, err))?;
            removed.push(path);
        }
    }

    Ok(removed)
}

fn remove_matching_train_run_indexes(
    root: &Path,
    adapter_ref: &AdapterRef,
) -> KernelResult<Vec<PathBuf>> {
    let mut removed = Vec::new();
    if !root.exists() {
        return Ok(removed);
    }

    for entry in fs::read_dir(root).map_err(|err| {
        path_error(
            "read training-run adapter source-index directory",
            root,
            err,
        )
    })? {
        let entry = entry.map_err(|err| {
            adapter_store_error(format!(
                "read entry in training-run adapter source-index directory `{}` failed: {err}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }

        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read training-run source index", &path, err))?;
        let index: TrainRunAdapterSourceIndex = toml::from_str(&body).map_err(|err| {
            adapter_store_error(format!(
                "parse training-run source index `{}` failed: {err}",
                path.display()
            ))
        })?;

        if index.adapter_ref == *adapter_ref {
            fs::remove_file(&path).map_err(|err| {
                path_error("remove training-run adapter source index", &path, err)
            })?;
            removed.push(path);
        }
    }

    Ok(removed)
}

fn remove_dir_if_empty(path: &Path) -> KernelResult<()> {
    if fs::read_dir(path)
        .map_err(|err| path_error("read source-index directory", path, err))?
        .next()
        .is_none()
    {
        fs::remove_dir(path)
            .map_err(|err| path_error("remove empty source-index directory", path, err))?;
    }

    Ok(())
}
