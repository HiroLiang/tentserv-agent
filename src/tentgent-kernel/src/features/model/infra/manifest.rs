use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::features::model::domain::{ModelManifest, ModelManifestEntry};
use crate::features::model::ports::ModelManifestBuilder;
use crate::foundation::error::KernelResult;

use super::error::{model_store_error, path_error};

/// Builds canonical manifests from staged model source directories.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdModelManifestBuilder;

impl ModelManifestBuilder for StdModelManifestBuilder {
    fn build_manifest(&self, source_root: &Path) -> KernelResult<ModelManifest> {
        if !source_root.exists() {
            return Err(model_store_error(format!(
                "model source root does not exist: `{}`",
                source_root.display()
            )));
        }

        if !source_root.is_dir() {
            return Err(model_store_error(format!(
                "model source root is not a directory: `{}`",
                source_root.display()
            )));
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(source_root) {
            let entry = entry.map_err(|err| {
                model_store_error(format!(
                    "walk model source `{}` failed: {err}",
                    source_root.display()
                ))
            })?;

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|err| path_error("read model source metadata", path, err))?;
            let relative = path.strip_prefix(source_root).map_err(|err| {
                model_store_error(format!(
                    "resolve relative path for `{}` failed: {err}",
                    path.display()
                ))
            })?;

            files.push(ModelManifestEntry {
                relative_path: normalize_relative_path(relative),
                size_bytes: metadata.len(),
                sha256: sha256_file(path)?,
            });
        }

        if files.is_empty() {
            return Err(model_store_error(format!(
                "model source root contains no regular files: `{}`",
                source_root.display()
            )));
        }

        Ok(ModelManifest { files }.sorted())
    }
}

fn sha256_file(path: &Path) -> KernelResult<String> {
    let file = File::open(path).map_err(|err| path_error("open model source file", path, err))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| path_error("read model source file", path, err))?;
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
