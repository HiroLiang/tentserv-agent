use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::features::dataset::domain::{DatasetManifest, DatasetManifestEntry};
use crate::features::dataset::ports::DatasetManifestBuilder;
use crate::foundation::error::KernelResult;

use super::error::{dataset_store_error, path_error};

/// Builds canonical manifests from staged dataset source directories.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDatasetManifestBuilder;

impl DatasetManifestBuilder for StdDatasetManifestBuilder {
    fn build_manifest(&self, source_root: &Path) -> KernelResult<DatasetManifest> {
        build_manifest_from_root(source_root)
    }
}

pub(super) fn build_manifest_from_root(source_root: &Path) -> KernelResult<DatasetManifest> {
    if !source_root.exists() {
        return Err(dataset_store_error(format!(
            "dataset source root does not exist: `{}`",
            source_root.display()
        )));
    }

    if !source_root.is_dir() {
        return Err(dataset_store_error(format!(
            "dataset source root is not a directory: `{}`",
            source_root.display()
        )));
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(source_root) {
        let entry = entry.map_err(|err| {
            dataset_store_error(format!(
                "walk dataset source `{}` failed: {err}",
                source_root.display()
            ))
        })?;

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| path_error("read dataset source metadata", path, err))?;
        let relative = path.strip_prefix(source_root).map_err(|err| {
            dataset_store_error(format!(
                "resolve relative path for `{}` failed: {err}",
                path.display()
            ))
        })?;

        files.push(DatasetManifestEntry {
            relative_path: normalize_relative_path(relative),
            size_bytes: metadata.len(),
            sha256: sha256_file(path)?,
        });
    }

    if files.is_empty() {
        return Err(dataset_store_error(format!(
            "dataset source root contains no regular files: `{}`",
            source_root.display()
        )));
    }

    Ok(DatasetManifest { files }.sorted())
}

fn sha256_file(path: &Path) -> KernelResult<String> {
    let file = File::open(path).map_err(|err| path_error("open dataset source file", path, err))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| path_error("read dataset source file", path, err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
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
