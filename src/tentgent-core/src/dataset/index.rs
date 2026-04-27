use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    error::DatasetError,
    store::{DatasetMetadata, DatasetStorePaths},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSourceIndex {
    pub dataset_ref: String,
    pub short_ref: String,
    pub source_path: String,
    pub imported_at: String,
}

pub fn write_local_index(
    paths: &DatasetStorePaths,
    metadata: &DatasetMetadata,
    source_path: &Path,
) -> Result<PathBuf, DatasetError> {
    let index = LocalSourceIndex {
        dataset_ref: metadata.dataset_ref.clone(),
        short_ref: metadata.short_ref.clone(),
        source_path: source_path.display().to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let path = paths
        .local_index_dir
        .join(format!("{}.toml", metadata.dataset_ref));
    write_toml(path.clone(), &index)?;
    Ok(path)
}

pub fn remove_indexes_for_dataset_ref(
    paths: &DatasetStorePaths,
    dataset_ref: &str,
) -> Result<Vec<PathBuf>, DatasetError> {
    let mut removed = Vec::new();
    let local_index = paths.local_index_dir.join(format!("{dataset_ref}.toml"));

    if local_index.exists() {
        fs::remove_file(&local_index)?;
        removed.push(local_index);
    }

    Ok(removed)
}

fn write_toml<T: Serialize>(path: PathBuf, value: &T) -> Result<(), DatasetError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let body = toml::to_string_pretty(value)?;
    fs::write(&path, body)?;
    Ok(())
}
