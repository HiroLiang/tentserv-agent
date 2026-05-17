use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::features::adapter::domain::{AdapterManifest, AdapterManifestEntry};
use crate::features::adapter::ports::AdapterManifestBuilder;
use crate::foundation::error::KernelResult;

use super::error::{adapter_store_error, path_error};

/// Builds canonical manifests from staged adapter source directories.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdAdapterManifestBuilder;

impl AdapterManifestBuilder for StdAdapterManifestBuilder {
    fn build_manifest(&self, source_root: &Path) -> KernelResult<AdapterManifest> {
        if !source_root.exists() {
            return Err(adapter_store_error(format!(
                "adapter source root does not exist: `{}`",
                source_root.display()
            )));
        }

        if !source_root.is_dir() {
            return Err(adapter_store_error(format!(
                "adapter source root is not a directory: `{}`",
                source_root.display()
            )));
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(source_root) {
            let entry = entry.map_err(|err| {
                adapter_store_error(format!(
                    "walk adapter source `{}` failed: {err}",
                    source_root.display()
                ))
            })?;

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|err| path_error("read adapter source metadata", path, err))?;
            let relative = path.strip_prefix(source_root).map_err(|err| {
                adapter_store_error(format!(
                    "resolve relative path for `{}` failed: {err}",
                    path.display()
                ))
            })?;

            files.push(AdapterManifestEntry {
                relative_path: normalize_relative_path(relative),
                size_bytes: metadata.len(),
                sha256: sha256_file(path)?,
            });
        }

        if files.is_empty() {
            return Err(adapter_store_error(format!(
                "adapter source root contains no regular files: `{}`",
                source_root.display()
            )));
        }

        Ok(AdapterManifest { files }.sorted())
    }
}

fn sha256_file(path: &Path) -> KernelResult<String> {
    let file = File::open(path).map_err(|err| path_error("open adapter source file", path, err))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| path_error("read adapter source file", path, err))?;
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
