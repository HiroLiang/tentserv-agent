use std::fs;
use std::path::{Path, PathBuf};

use crate::features::model::domain::{
    HfModelSourceIndex, LocalModelSourceIndex, ModelRef, ModelStoreLayout,
};
use crate::features::model::ports::ModelSourceIndexStore;
use crate::foundation::error::KernelResult;

use super::error::{model_store_error, path_error};

/// Filesystem-backed source-index store for canonical models.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileModelSourceIndexStore;

impl ModelSourceIndexStore for FileModelSourceIndexStore {
    fn save_local_source_index(
        &self,
        layout: &ModelStoreLayout,
        index: &LocalModelSourceIndex,
    ) -> KernelResult<PathBuf> {
        fs::create_dir_all(&layout.local_index_dir).map_err(|err| {
            path_error(
                "create local model source-index directory",
                &layout.local_index_dir,
                err,
            )
        })?;
        let path = layout.local_index_path(&index.model_ref);
        write_toml(&path, index)?;
        Ok(path)
    }

    fn save_hf_source_index(
        &self,
        layout: &ModelStoreLayout,
        index: &HfModelSourceIndex,
    ) -> KernelResult<PathBuf> {
        let repo_dir = layout.hf_index_dir_for_repo(&index.source_repo);
        fs::create_dir_all(&repo_dir).map_err(|err| {
            path_error("create Hugging Face source-index directory", &repo_dir, err)
        })?;
        let path = layout.hf_index_path(&index.source_repo, &index.source_revision);
        write_toml(&path, index)?;
        Ok(path)
    }

    fn remove_source_indexes(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<PathBuf>> {
        let mut removed = Vec::new();

        let local_path = layout.local_index_path(model_ref);
        if local_path.exists() {
            fs::remove_file(&local_path)
                .map_err(|err| path_error("remove local model source index", &local_path, err))?;
            removed.push(local_path);
        }

        removed.extend(remove_matching_hf_indexes(&layout.hf_index_dir, model_ref)?);
        Ok(removed)
    }
}

fn write_toml<T: serde::Serialize>(path: &Path, value: &T) -> KernelResult<()> {
    let body = toml::to_string_pretty(value)
        .map_err(|err| model_store_error(format!("serialize model source index failed: {err}")))?;
    fs::write(path, body).map_err(|err| path_error("write model source index", path, err))
}

fn remove_matching_hf_indexes(root: &Path, model_ref: &ModelRef) -> KernelResult<Vec<PathBuf>> {
    let mut removed = Vec::new();
    if !root.exists() {
        return Ok(removed);
    }

    for entry in fs::read_dir(root)
        .map_err(|err| path_error("read Hugging Face source-index directory", root, err))?
    {
        let entry = entry.map_err(|err| {
            model_store_error(format!(
                "read entry in Hugging Face source-index directory `{}` failed: {err}",
                root.display()
            ))
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| path_error("read source-index entry type", &path, err))?;

        if file_type.is_dir() {
            removed.extend(remove_matching_hf_indexes(&path, model_ref)?);
            if fs::read_dir(&path)
                .map_err(|err| path_error("read source-index directory", &path, err))?
                .next()
                .is_none()
            {
                fs::remove_dir(&path)
                    .map_err(|err| path_error("remove empty source-index directory", &path, err))?;
            }
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }

        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read HF source index", &path, err))?;
        let index: HfModelSourceIndex = toml::from_str(&body).map_err(|err| {
            model_store_error(format!(
                "parse HF source index `{}` failed: {err}",
                path.display()
            ))
        })?;

        if index.model_ref == *model_ref {
            fs::remove_file(&path)
                .map_err(|err| path_error("remove HF source index", &path, err))?;
            removed.push(path);
        }
    }

    Ok(removed)
}
