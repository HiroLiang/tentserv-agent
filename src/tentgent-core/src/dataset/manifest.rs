use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::{error::DatasetError, hash};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDocument {
    pub files: Vec<ManifestEntry>,
}

impl ManifestDocument {
    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, DatasetError> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn pretty_json_bytes(&self) -> Result<Vec<u8>, DatasetError> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|entry| entry.size_bytes).sum()
    }
}

pub fn read_manifest(path: &Path) -> Result<ManifestDocument, DatasetError> {
    let body = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&body)?)
}

pub fn build_manifest(root: &Path) -> Result<ManifestDocument, DatasetError> {
    if !root.exists() {
        return Err(DatasetError::MissingPath(root.to_path_buf()));
    }

    if !root.is_dir() {
        return Err(DatasetError::UnsupportedPath(root.to_path_buf()));
    }

    let mut files = Vec::new();

    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|err| DatasetError::Walk {
            path: root.to_path_buf(),
            message: err.to_string(),
        })?;

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let metadata = entry.metadata().map_err(|err| DatasetError::Walk {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;
        let relative = path.strip_prefix(root).map_err(|err| DatasetError::Walk {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;

        files.push(ManifestEntry {
            relative_path: normalize_relative_path(relative),
            size_bytes: metadata.len(),
            sha256: hash::sha256_file(path)?,
        });
    }

    if files.is_empty() {
        return Err(DatasetError::UnsupportedLayout {
            reason: format!("no regular files were found under `{}`", root.display()),
        });
    }

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(ManifestDocument { files })
}

fn normalize_relative_path(path: &Path) -> String {
    let normalized = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");

    if normalized.is_empty() {
        PathBuf::from(path).display().to_string()
    } else {
        normalized
    }
}
