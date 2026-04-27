use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    error::AdapterError,
    store::{AdapterMetadata, AdapterStorePaths},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSourceIndex {
    pub adapter_ref: String,
    pub short_ref: String,
    pub source_path: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSourceIndex {
    pub adapter_ref: String,
    pub short_ref: String,
    pub source_repo: String,
    pub source_revision: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseModelIndex {
    pub adapter_ref: String,
    pub short_ref: String,
    pub base_model_ref: String,
    pub adapter_format: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainRunSourceIndex {
    pub adapter_ref: String,
    pub short_ref: String,
    pub training_run_ref: String,
    pub training_dataset_ref: String,
    pub training_config_ref: String,
    pub imported_at: String,
}

pub fn write_local_index(
    paths: &AdapterStorePaths,
    metadata: &AdapterMetadata,
    source_path: &Path,
) -> Result<PathBuf, AdapterError> {
    let index = LocalSourceIndex {
        adapter_ref: metadata.adapter_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        source_path: source_path.display().to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let path = paths
        .local_index_dir
        .join(format!("{}.toml", metadata.adapter_ref));
    write_toml(path.clone(), &index)?;
    Ok(path)
}

pub fn write_hf_index(
    paths: &AdapterStorePaths,
    metadata: &AdapterMetadata,
    repo_id: &str,
    revision: &str,
) -> Result<PathBuf, AdapterError> {
    let repo_dir = paths.hf_index_dir.join(escape_repo_id(repo_id));
    fs::create_dir_all(&repo_dir)?;

    let index = HfSourceIndex {
        adapter_ref: metadata.adapter_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        source_repo: repo_id.to_string(),
        source_revision: revision.to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let path = repo_dir.join(format!("{revision}.toml"));
    write_toml(path.clone(), &index)?;
    Ok(path)
}

pub fn write_base_index(
    paths: &AdapterStorePaths,
    metadata: &AdapterMetadata,
    base_model_ref: &str,
) -> Result<PathBuf, AdapterError> {
    let index = BaseModelIndex {
        adapter_ref: metadata.adapter_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        base_model_ref: base_model_ref.to_string(),
        adapter_format: metadata.adapter_format.as_str().to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let path = paths
        .by_base_dir
        .join(base_model_ref)
        .join(format!("{}.toml", metadata.adapter_ref));
    write_toml(path.clone(), &index)?;
    Ok(path)
}

pub fn write_train_run_index(
    paths: &AdapterStorePaths,
    metadata: &AdapterMetadata,
    run_ref: &str,
    dataset_ref: &str,
    config_ref: &str,
) -> Result<PathBuf, AdapterError> {
    let index = TrainRunSourceIndex {
        adapter_ref: metadata.adapter_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        training_run_ref: run_ref.to_string(),
        training_dataset_ref: dataset_ref.to_string(),
        training_config_ref: config_ref.to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let path = paths.train_run_index_dir.join(format!("{run_ref}.toml"));
    write_toml(path.clone(), &index)?;
    Ok(path)
}

pub fn remove_base_index(
    paths: &AdapterStorePaths,
    adapter_ref: &str,
    base_model_ref: &str,
) -> Result<Option<PathBuf>, AdapterError> {
    let path = paths
        .by_base_dir
        .join(base_model_ref)
        .join(format!("{adapter_ref}.toml"));
    if path.exists() {
        fs::remove_file(&path)?;
        return Ok(Some(path));
    }

    Ok(None)
}

pub fn remove_indexes_for_adapter(
    paths: &AdapterStorePaths,
    metadata: &AdapterMetadata,
) -> Result<Vec<PathBuf>, AdapterError> {
    let mut removed = Vec::new();

    let local_path = paths
        .local_index_dir
        .join(format!("{}.toml", metadata.adapter_ref));
    remove_file_if_exists(&mut removed, local_path)?;
    removed.extend(remove_matching_hf_indexes(
        &paths.hf_index_dir,
        &metadata.adapter_ref,
    )?);
    removed.extend(remove_matching_train_run_indexes(
        &paths.train_run_index_dir,
        &metadata.adapter_ref,
    )?);

    if let Some(base_model_ref) = metadata.base_model_ref.as_deref() {
        if let Some(path) = remove_base_index(paths, &metadata.adapter_ref, base_model_ref)? {
            removed.push(path);
        }
    }

    Ok(removed)
}

fn remove_matching_train_run_indexes(
    root: &Path,
    adapter_ref: &str,
) -> Result<Vec<PathBuf>, AdapterError> {
    let mut removed = Vec::new();

    if !root.exists() {
        return Ok(removed);
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }

        let body = fs::read_to_string(&path)?;
        let index: TrainRunSourceIndex =
            toml::from_str(&body).map_err(|err| AdapterError::MetadataParse {
                path: path.clone(),
                message: err.to_string(),
            })?;

        if index.adapter_ref == adapter_ref {
            fs::remove_file(&path)?;
            removed.push(path);
        }
    }

    Ok(removed)
}

pub fn escape_repo_id(repo_id: &str) -> String {
    repo_id.replace('/', "--")
}

fn write_toml<T: Serialize>(path: PathBuf, value: &T) -> Result<(), AdapterError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let body = toml::to_string_pretty(value)?;
    fs::write(&path, body)?;
    Ok(())
}

fn remove_file_if_exists(removed: &mut Vec<PathBuf>, path: PathBuf) -> Result<(), AdapterError> {
    if path.exists() {
        fs::remove_file(&path)?;
        removed.push(path);
    }

    Ok(())
}

fn remove_matching_hf_indexes(
    root: &Path,
    adapter_ref: &str,
) -> Result<Vec<PathBuf>, AdapterError> {
    let mut removed = Vec::new();

    if !root.exists() {
        return Ok(removed);
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if entry.file_type()?.is_dir() {
            removed.extend(remove_matching_hf_indexes(&path, adapter_ref)?);

            if fs::read_dir(&path)?.next().is_none() {
                fs::remove_dir(&path)?;
            }
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }

        let body = fs::read_to_string(&path)?;
        let index: HfSourceIndex =
            toml::from_str(&body).map_err(|err| AdapterError::MetadataParse {
                path: path.clone(),
                message: err.to_string(),
            })?;

        if index.adapter_ref == adapter_ref {
            fs::remove_file(&path)?;
            removed.push(path);
        }
    }

    Ok(removed)
}
