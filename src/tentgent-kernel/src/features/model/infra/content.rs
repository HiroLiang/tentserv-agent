use std::fs;
use std::path::PathBuf;

use crate::features::model::domain::{ModelFormat, ModelRef, ModelStoreLayout};
use crate::features::model::ports::{ModelContentStore, StagedModelSource};
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Filesystem-backed canonical model content store.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileModelContentStore;

impl ModelContentStore for FileModelContentStore {
    fn model_content_exists(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<bool> {
        Ok(layout.model_dir(model_ref).exists())
    }

    fn install_staged_source(
        &self,
        layout: &ModelStoreLayout,
        staged: &StagedModelSource,
        model_ref: &ModelRef,
        format: ModelFormat,
    ) -> KernelResult<PathBuf> {
        let variant_dir = layout.variant_dir(model_ref, format);
        fs::create_dir_all(&variant_dir)
            .map_err(|err| path_error("create model variant directory", &variant_dir, err))?;

        let destination = layout.variant_source_dir(model_ref, format);
        fs::rename(&staged.source_dir, &destination).map_err(|err| {
            path_error(
                "move staged model source into store",
                &staged.source_dir,
                err,
            )
        })?;
        Ok(destination)
    }

    fn remove_model_content(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<()> {
        let path = layout.model_dir(model_ref);
        if path.exists() {
            fs::remove_dir_all(&path)
                .map_err(|err| path_error("remove canonical model directory", &path, err))?;
        }
        Ok(())
    }
}
