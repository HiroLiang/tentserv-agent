use std::fs;

use crate::features::model::domain::{
    ModelInspection, ModelManifest, ModelMetadata, ModelRef, ModelRefSelector, ModelStoreLayout,
    ModelSummary, ModelVariantMetadata,
};
use crate::features::model::ports::ModelCatalogStore;
use crate::foundation::error::KernelResult;

use super::error::{model_store_error, path_error};

/// Filesystem-backed model metadata catalog.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileModelCatalogStore;

impl ModelCatalogStore for FileModelCatalogStore {
    fn list_models(&self, layout: &ModelStoreLayout) -> KernelResult<Vec<ModelSummary>> {
        let mut models = Vec::new();
        if !layout.store_dir.exists() {
            return Ok(models);
        }

        for entry in fs::read_dir(&layout.store_dir)
            .map_err(|err| path_error("read model store directory", &layout.store_dir, err))?
        {
            let entry = entry.map_err(|err| {
                model_store_error(format!(
                    "read entry in model store `{}` failed: {err}",
                    layout.store_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error("read model store entry type", entry.path().as_path(), err)
            })?;
            if !file_type.is_dir() {
                continue;
            }

            let model_ref =
                ModelRef::parse(entry.file_name().to_string_lossy()).map_err(|err| {
                    model_store_error(format!(
                        "invalid model directory name `{}`: {err}",
                        entry.file_name().to_string_lossy()
                    ))
                })?;
            let metadata = self.load_model_metadata(layout, &model_ref)?;
            models.push(ModelSummary {
                store_path: layout.model_dir(&model_ref),
                metadata,
            });
        }

        models.sort_by(|left, right| left.metadata.short_ref.cmp(&right.metadata.short_ref));
        Ok(models)
    }

    fn inspect_model(
        &self,
        layout: &ModelStoreLayout,
        selector: &ModelRefSelector,
    ) -> KernelResult<ModelInspection> {
        let metadata = resolve_metadata(self, layout, selector)?;
        let store_path = layout.model_dir(&metadata.model_ref);
        let manifest_path = layout.manifest_path(&metadata.model_ref);
        let variant_source_path =
            layout.variant_source_dir(&metadata.model_ref, metadata.primary_format);

        Ok(ModelInspection {
            metadata,
            store_path,
            manifest_path,
            variant_source_path,
        })
    }

    fn load_model_metadata(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<ModelMetadata> {
        let path = layout.model_metadata_path(model_ref);
        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read model metadata", &path, err))?;
        toml::from_str(&body).map_err(|err| {
            model_store_error(format!(
                "parse model metadata `{}` failed: {err}",
                path.display()
            ))
        })
    }

    fn save_model_metadata(
        &self,
        layout: &ModelStoreLayout,
        metadata: &ModelMetadata,
    ) -> KernelResult<()> {
        let path = layout.model_metadata_path(&metadata.model_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| path_error("create model metadata parent directory", parent, err))?;
        }
        let body = toml::to_string_pretty(metadata)
            .map_err(|err| model_store_error(format!("serialize model metadata failed: {err}")))?;
        fs::write(&path, body).map_err(|err| path_error("write model metadata", &path, err))?;
        Ok(())
    }

    fn save_model_manifest(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        manifest: &ModelManifest,
    ) -> KernelResult<()> {
        let path = layout.manifest_path(model_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| path_error("create model manifest parent directory", parent, err))?;
        }
        let body = serde_json::to_vec_pretty(manifest)
            .map_err(|err| model_store_error(format!("serialize model manifest failed: {err}")))?;
        fs::write(&path, body).map_err(|err| path_error("write model manifest", &path, err))?;
        Ok(())
    }

    fn save_variant_metadata(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
        variant: &ModelVariantMetadata,
    ) -> KernelResult<()> {
        let path = layout.variant_metadata_path(model_ref, variant.format);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                path_error("create variant metadata parent directory", parent, err)
            })?;
        }
        let body = toml::to_string_pretty(variant).map_err(|err| {
            model_store_error(format!("serialize variant metadata failed: {err}"))
        })?;
        fs::write(&path, body).map_err(|err| path_error("write variant metadata", &path, err))?;
        Ok(())
    }
}

fn resolve_metadata(
    store: &FileModelCatalogStore,
    layout: &ModelStoreLayout,
    selector: &ModelRefSelector,
) -> KernelResult<ModelMetadata> {
    if selector.is_full_ref() {
        let model_ref =
            ModelRef::parse(selector.as_str()).map_err(|err| model_store_error(err.to_string()))?;
        let path = layout.model_metadata_path(&model_ref);
        if path.exists() {
            return store.load_model_metadata(layout, &model_ref);
        }
    }

    let mut matches = Vec::new();
    if !layout.store_dir.exists() {
        return Err(not_found(selector));
    }

    for entry in fs::read_dir(&layout.store_dir)
        .map_err(|err| path_error("read model store directory", &layout.store_dir, err))?
    {
        let entry = entry.map_err(|err| {
            model_store_error(format!(
                "read entry in model store `{}` failed: {err}",
                layout.store_dir.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            path_error("read model store entry type", entry.path().as_path(), err)
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let entry_ref = entry.file_name().to_string_lossy().into_owned();
        if entry_ref.starts_with(selector.as_str()) {
            let model_ref = ModelRef::parse(entry_ref.as_str())
                .map_err(|err| model_store_error(err.to_string()))?;
            matches.push(store.load_model_metadata(layout, &model_ref)?);
        }
    }

    match matches.len() {
        0 => Err(not_found(selector)),
        1 => Ok(matches.remove(0)),
        _ => Err(model_store_error(format!(
            "model reference `{}` is ambiguous; multiple stored models share that prefix",
            selector.as_str()
        ))),
    }
}

fn not_found(selector: &ModelRefSelector) -> crate::foundation::error::KernelError {
    model_store_error(format!(
        "model reference `{}` was not found",
        selector.as_str()
    ))
}
