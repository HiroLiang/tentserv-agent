use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::features::dataset::domain::{DatasetStoreLayout, SOURCE_DIRNAME};
use crate::features::dataset::ports::{DatasetSourceStager, StagedDatasetSource};
use crate::foundation::error::KernelResult;

use super::error::{dataset_store_error, path_error};

/// Filesystem implementation for temporary dataset import/diff staging.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDatasetSourceStager;

impl DatasetSourceStager for StdDatasetSourceStager {
    fn create_staging_source(
        &self,
        layout: &DatasetStoreLayout,
        prefix: &str,
    ) -> KernelResult<StagedDatasetSource> {
        let prefix = normalize_staging_prefix(prefix);
        let staging_root = layout.staging_dir.join(format!(
            "{}-{}-{}",
            prefix,
            current_millis(),
            std::process::id()
        ));
        let source_dir = staging_root.join(SOURCE_DIRNAME);

        fs::create_dir_all(&source_dir)
            .map_err(|err| path_error("create staging source directory", &source_dir, err))?;

        Ok(StagedDatasetSource {
            staging_root,
            source_dir,
        })
    }

    fn copy_local_source(
        &self,
        input_path: &Path,
        staged: &StagedDatasetSource,
    ) -> KernelResult<()> {
        if !input_path.exists() {
            return Err(dataset_store_error(format!(
                "dataset source path does not exist: `{}`",
                input_path.display()
            )));
        }

        if input_path.is_file() {
            if !is_jsonl_file(input_path) {
                return Err(dataset_store_error(format!(
                    "dataset source file must have .jsonl extension: `{}`",
                    input_path.display()
                )));
            }
            let file_name = input_path.file_name().ok_or_else(|| {
                dataset_store_error(format!(
                    "dataset source path has no file name: `{}`",
                    input_path.display()
                ))
            })?;
            let destination = staged.source_dir.join(file_name);
            fs::copy(input_path, &destination)
                .map_err(|err| path_error("copy dataset source file", input_path, err))?;
            return Ok(());
        }

        if !input_path.is_dir() {
            return Err(dataset_store_error(format!(
                "dataset source path is not a regular JSONL file or directory: `{}`",
                input_path.display()
            )));
        }

        copy_dir_contents(input_path, &staged.source_dir)
    }

    fn discard_staging(&self, staged: &StagedDatasetSource) -> KernelResult<()> {
        if staged.staging_root.exists() {
            fs::remove_dir_all(&staged.staging_root)
                .map_err(|err| path_error("remove staging root", &staged.staging_root, err))?;
        }

        Ok(())
    }
}

pub(super) fn copy_dir_contents(input_path: &Path, source_dir: &Path) -> KernelResult<()> {
    for entry in WalkDir::new(input_path) {
        let entry = entry.map_err(|err| {
            dataset_store_error(format!(
                "walk dataset source `{}` failed: {err}",
                input_path.display()
            ))
        })?;
        let path = entry.path();
        let relative = path.strip_prefix(input_path).map_err(|err| {
            dataset_store_error(format!(
                "resolve relative path for `{}` failed: {err}",
                path.display()
            ))
        })?;

        if relative.as_os_str().is_empty() {
            continue;
        }

        let destination = source_dir.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|err| {
                path_error("create copied dataset source directory", &destination, err)
            })?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    path_error("create copied dataset source parent directory", parent, err)
                })?;
            }
            fs::copy(path, &destination)
                .map_err(|err| path_error("copy dataset source file", path, err))?;
        }
    }

    Ok(())
}

fn is_jsonl_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
}

fn normalize_staging_prefix(prefix: &str) -> String {
    let normalized = prefix
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if normalized.trim_matches('-').is_empty() {
        "dataset".to_string()
    } else {
        normalized
    }
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
