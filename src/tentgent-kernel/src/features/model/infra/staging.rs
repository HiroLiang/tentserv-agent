use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::features::model::domain::{ModelImportMethod, ModelStoreLayout, SOURCE_DIRNAME};
use crate::features::model::ports::{ModelSourceStager, StagedModelSource};
use crate::foundation::error::KernelResult;

use super::error::{model_store_error, path_error};

/// Filesystem implementation for temporary model import staging.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdModelSourceStager;

impl ModelSourceStager for StdModelSourceStager {
    fn create_staging_source(
        &self,
        layout: &ModelStoreLayout,
        method: ModelImportMethod,
    ) -> KernelResult<StagedModelSource> {
        let staging_root = layout.staging_dir.join(format!(
            "{}-{}-{}",
            method.as_str(),
            current_millis(),
            std::process::id()
        ));
        let source_dir = staging_root.join(SOURCE_DIRNAME);

        fs::create_dir_all(&source_dir)
            .map_err(|err| path_error("create staging source directory", &source_dir, err))?;

        Ok(StagedModelSource {
            staging_root,
            source_dir,
        })
    }

    fn copy_local_source(&self, input_path: &Path, staged: &StagedModelSource) -> KernelResult<()> {
        if !input_path.exists() {
            return Err(model_store_error(format!(
                "model source path does not exist: `{}`",
                input_path.display()
            )));
        }

        if input_path.is_file() {
            let file_name = input_path.file_name().ok_or_else(|| {
                model_store_error(format!(
                    "model source path has no file name: `{}`",
                    input_path.display()
                ))
            })?;
            let destination = staged.source_dir.join(file_name);
            fs::copy(input_path, &destination)
                .map_err(|err| path_error("copy model source file", input_path, err))?;
            return Ok(());
        }

        if !input_path.is_dir() {
            return Err(model_store_error(format!(
                "model source path is not a regular file or directory: `{}`",
                input_path.display()
            )));
        }

        copy_dir_contents(input_path, &staged.source_dir)
    }

    fn discard_staging(&self, staged: &StagedModelSource) -> KernelResult<()> {
        if staged.staging_root.exists() {
            fs::remove_dir_all(&staged.staging_root)
                .map_err(|err| path_error("remove staging root", &staged.staging_root, err))?;
        }

        Ok(())
    }
}

fn copy_dir_contents(input_path: &Path, source_dir: &Path) -> KernelResult<()> {
    for entry in WalkDir::new(input_path) {
        let entry = entry.map_err(|err| {
            model_store_error(format!(
                "walk model source `{}` failed: {err}",
                input_path.display()
            ))
        })?;
        let path = entry.path();
        let relative = path.strip_prefix(input_path).map_err(|err| {
            model_store_error(format!(
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
                path_error("create copied model source directory", &destination, err)
            })?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    path_error("create copied model source parent directory", parent, err)
                })?;
            }
            fs::copy(path, &destination)
                .map_err(|err| path_error("copy model source file", path, err))?;
        }
    }

    Ok(())
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
