use std::fs;

use crate::features::dataset::domain::{
    DatasetInspection, DatasetManifest, DatasetMetadata, DatasetRef, DatasetRefSelector,
    DatasetStoreLayout, DatasetSummary,
};
use crate::features::dataset::ports::DatasetCatalogStore;
use crate::foundation::error::KernelResult;

use super::error::{dataset_store_error, path_error};

/// Filesystem-backed dataset metadata catalog.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileDatasetCatalogStore;

impl DatasetCatalogStore for FileDatasetCatalogStore {
    fn list_datasets(&self, layout: &DatasetStoreLayout) -> KernelResult<Vec<DatasetSummary>> {
        let mut datasets = Vec::new();
        if !layout.store_dir.exists() {
            return Ok(datasets);
        }

        for entry in fs::read_dir(&layout.store_dir)
            .map_err(|err| path_error("read dataset store directory", &layout.store_dir, err))?
        {
            let entry = entry.map_err(|err| {
                dataset_store_error(format!(
                    "read entry in dataset store `{}` failed: {err}",
                    layout.store_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error("read dataset store entry type", entry.path().as_path(), err)
            })?;
            if !file_type.is_dir() {
                continue;
            }

            let dataset_ref =
                DatasetRef::parse(entry.file_name().to_string_lossy()).map_err(|err| {
                    dataset_store_error(format!(
                        "invalid dataset directory name `{}`: {err}",
                        entry.file_name().to_string_lossy()
                    ))
                })?;
            let metadata = self.load_dataset_metadata(layout, &dataset_ref)?;
            datasets.push(DatasetSummary {
                store_path: layout.dataset_dir(&dataset_ref),
                metadata,
            });
        }

        datasets.sort_by(|left, right| left.metadata.short_ref.cmp(&right.metadata.short_ref));
        Ok(datasets)
    }

    fn inspect_dataset(
        &self,
        layout: &DatasetStoreLayout,
        selector: &DatasetRefSelector,
    ) -> KernelResult<DatasetInspection> {
        let metadata = resolve_metadata(self, layout, selector)?;
        let store_path = layout.dataset_dir(&metadata.dataset_ref);
        let manifest_path = layout.manifest_path(&metadata.dataset_ref);
        let source_path = layout.source_dir(&metadata.dataset_ref);

        Ok(DatasetInspection {
            metadata,
            store_path,
            manifest_path,
            source_path,
        })
    }

    fn load_dataset_metadata(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<DatasetMetadata> {
        let path = layout.dataset_metadata_path(dataset_ref);
        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read dataset metadata", &path, err))?;
        toml::from_str(&body).map_err(|err| {
            dataset_store_error(format!(
                "parse dataset metadata `{}` failed: {err}",
                path.display()
            ))
        })
    }

    fn save_dataset_metadata(
        &self,
        layout: &DatasetStoreLayout,
        metadata: &DatasetMetadata,
    ) -> KernelResult<()> {
        let path = layout.dataset_metadata_path(&metadata.dataset_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                path_error("create dataset metadata parent directory", parent, err)
            })?;
        }
        let body = toml::to_string_pretty(metadata).map_err(|err| {
            dataset_store_error(format!("serialize dataset metadata failed: {err}"))
        })?;
        fs::write(&path, body).map_err(|err| path_error("write dataset metadata", &path, err))?;
        Ok(())
    }

    fn save_dataset_manifest(
        &self,
        layout: &DatasetStoreLayout,
        dataset_ref: &DatasetRef,
        manifest: &DatasetManifest,
    ) -> KernelResult<()> {
        let path = layout.manifest_path(dataset_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                path_error("create dataset manifest parent directory", parent, err)
            })?;
        }
        let body = serde_json::to_vec_pretty(manifest).map_err(|err| {
            dataset_store_error(format!("serialize dataset manifest failed: {err}"))
        })?;
        fs::write(&path, body).map_err(|err| path_error("write dataset manifest", &path, err))?;
        Ok(())
    }
}

fn resolve_metadata(
    store: &FileDatasetCatalogStore,
    layout: &DatasetStoreLayout,
    selector: &DatasetRefSelector,
) -> KernelResult<DatasetMetadata> {
    if selector.is_full_ref() {
        let dataset_ref = DatasetRef::parse(selector.as_str())
            .map_err(|err| dataset_store_error(err.to_string()))?;
        let path = layout.dataset_metadata_path(&dataset_ref);
        if path.exists() {
            return store.load_dataset_metadata(layout, &dataset_ref);
        }
    }

    let mut matches = Vec::new();
    if !layout.store_dir.exists() {
        return Err(not_found(selector));
    }

    for entry in fs::read_dir(&layout.store_dir)
        .map_err(|err| path_error("read dataset store directory", &layout.store_dir, err))?
    {
        let entry = entry.map_err(|err| {
            dataset_store_error(format!(
                "read entry in dataset store `{}` failed: {err}",
                layout.store_dir.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            path_error("read dataset store entry type", entry.path().as_path(), err)
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let entry_ref = entry.file_name().to_string_lossy().into_owned();
        if entry_ref.starts_with(selector.as_str()) {
            let dataset_ref = DatasetRef::parse(entry_ref.as_str())
                .map_err(|err| dataset_store_error(err.to_string()))?;
            matches.push(store.load_dataset_metadata(layout, &dataset_ref)?);
        }
    }

    match matches.len() {
        0 => Err(not_found(selector)),
        1 => Ok(matches.remove(0)),
        _ => Err(dataset_store_error(format!(
            "dataset reference `{}` is ambiguous; multiple stored datasets share that prefix",
            selector.as_str()
        ))),
    }
}

fn not_found(selector: &DatasetRefSelector) -> crate::foundation::error::KernelError {
    dataset_store_error(format!(
        "dataset reference `{}` was not found",
        selector.as_str()
    ))
}
