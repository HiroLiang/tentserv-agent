use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    error::ModelError,
    store::{ModelMetadata, ModelStorePaths},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSourceIndex {
    pub model_ref: String,
    pub short_ref: String,
    pub source_path: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSourceIndex {
    pub model_ref: String,
    pub short_ref: String,
    pub source_repo: String,
    pub source_revision: String,
    pub imported_at: String,
}

pub fn write_local_index(
    paths: &ModelStorePaths,
    metadata: &ModelMetadata,
    source_path: &Path,
) -> Result<PathBuf, ModelError> {
    let index = LocalSourceIndex {
        model_ref: metadata.model_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        source_path: source_path.display().to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let path = paths
        .local_index_dir
        .join(format!("{}.toml", metadata.model_ref));
    write_toml(path.clone(), &index)?;
    Ok(path)
}

pub fn write_hf_index(
    paths: &ModelStorePaths,
    metadata: &ModelMetadata,
    repo_id: &str,
    revision: &str,
) -> Result<PathBuf, ModelError> {
    let repo_dir = paths.hf_index_dir.join(escape_repo_id(repo_id));
    fs::create_dir_all(&repo_dir)?;

    let index = HfSourceIndex {
        model_ref: metadata.model_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        source_repo: repo_id.to_string(),
        source_revision: revision.to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let path = repo_dir.join(format!("{revision}.toml"));
    write_toml(path.clone(), &index)?;
    Ok(path)
}

pub fn escape_repo_id(repo_id: &str) -> String {
    repo_id.replace('/', "--")
}

pub fn remove_indexes_for_model_ref(
    paths: &ModelStorePaths,
    model_ref: &str,
) -> Result<Vec<PathBuf>, ModelError> {
    let mut removed = Vec::new();

    let local_index = paths.local_index_dir.join(format!("{model_ref}.toml"));
    if local_index.exists() {
        fs::remove_file(&local_index)?;
        removed.push(local_index);
    }

    removed.extend(remove_matching_hf_indexes(&paths.hf_index_dir, model_ref)?);
    Ok(removed)
}

fn write_toml<T: Serialize>(path: PathBuf, value: &T) -> Result<(), ModelError> {
    let body = toml::to_string_pretty(value)?;
    fs::write(&path, body)?;
    Ok(())
}

fn remove_matching_hf_indexes(root: &Path, model_ref: &str) -> Result<Vec<PathBuf>, ModelError> {
    let mut removed = Vec::new();

    if !root.exists() {
        return Ok(removed);
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if entry.file_type()?.is_dir() {
            removed.extend(remove_matching_hf_indexes(&path, model_ref)?);

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
            toml::from_str(&body).map_err(|err| ModelError::MetadataParse {
                path: path.clone(),
                message: err.to_string(),
            })?;

        if index.model_ref == model_ref {
            fs::remove_file(&path)?;
            removed.push(path);
        }
    }

    Ok(removed)
}
