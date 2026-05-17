use std::fs;

use crate::features::adapter::domain::{
    AdapterInspection, AdapterManifest, AdapterMetadata, AdapterRef, AdapterRefSelector,
    AdapterStoreLayout, AdapterSummary,
};
use crate::features::adapter::ports::AdapterCatalogStore;
use crate::foundation::error::{KernelError, KernelResult};

use super::error::{adapter_store_error, path_error};

/// Filesystem-backed adapter metadata catalog.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileAdapterCatalogStore;

impl AdapterCatalogStore for FileAdapterCatalogStore {
    fn list_adapters(&self, layout: &AdapterStoreLayout) -> KernelResult<Vec<AdapterSummary>> {
        let mut adapters = Vec::new();
        if !layout.store_dir.exists() {
            return Ok(adapters);
        }

        for entry in fs::read_dir(&layout.store_dir)
            .map_err(|err| path_error("read adapter store directory", &layout.store_dir, err))?
        {
            let entry = entry.map_err(|err| {
                adapter_store_error(format!(
                    "read entry in adapter store `{}` failed: {err}",
                    layout.store_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error("read adapter store entry type", entry.path().as_path(), err)
            })?;
            if !file_type.is_dir() {
                continue;
            }

            let adapter_ref =
                AdapterRef::parse(entry.file_name().to_string_lossy()).map_err(|err| {
                    adapter_store_error(format!(
                        "invalid adapter directory name `{}`: {err}",
                        entry.file_name().to_string_lossy()
                    ))
                })?;
            let metadata = self.load_adapter_metadata(layout, &adapter_ref)?;
            adapters.push(AdapterSummary {
                store_path: layout.adapter_dir(&adapter_ref),
                metadata,
            });
        }

        adapters.sort_by(|left, right| left.metadata.short_ref.cmp(&right.metadata.short_ref));
        Ok(adapters)
    }

    fn inspect_adapter(
        &self,
        layout: &AdapterStoreLayout,
        selector: &AdapterRefSelector,
    ) -> KernelResult<AdapterInspection> {
        let metadata = resolve_metadata(self, layout, selector)?;
        let store_path = layout.adapter_dir(&metadata.adapter_ref);
        let manifest_path = layout.manifest_path(&metadata.adapter_ref);
        let source_path = layout.source_dir(&metadata.adapter_ref);

        Ok(AdapterInspection {
            metadata,
            store_path,
            manifest_path,
            source_path,
        })
    }

    fn load_adapter_metadata(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<AdapterMetadata> {
        let path = layout.adapter_metadata_path(adapter_ref);
        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read adapter metadata", &path, err))?;
        toml::from_str(&body).map_err(|err| {
            adapter_store_error(format!(
                "parse adapter metadata `{}` failed: {err}",
                path.display()
            ))
        })
    }

    fn save_adapter_metadata(
        &self,
        layout: &AdapterStoreLayout,
        metadata: &AdapterMetadata,
    ) -> KernelResult<()> {
        let path = layout.adapter_metadata_path(&metadata.adapter_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                path_error("create adapter metadata parent directory", parent, err)
            })?;
        }
        let body = toml::to_string_pretty(metadata).map_err(|err| {
            adapter_store_error(format!("serialize adapter metadata failed: {err}"))
        })?;
        fs::write(&path, body).map_err(|err| path_error("write adapter metadata", &path, err))?;
        Ok(())
    }

    fn save_adapter_manifest(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
        manifest: &AdapterManifest,
    ) -> KernelResult<()> {
        let path = layout.manifest_path(adapter_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                path_error("create adapter manifest parent directory", parent, err)
            })?;
        }
        let body = serde_json::to_vec_pretty(manifest).map_err(|err| {
            adapter_store_error(format!("serialize adapter manifest failed: {err}"))
        })?;
        fs::write(&path, body).map_err(|err| path_error("write adapter manifest", &path, err))?;
        Ok(())
    }
}

fn resolve_metadata(
    store: &FileAdapterCatalogStore,
    layout: &AdapterStoreLayout,
    selector: &AdapterRefSelector,
) -> KernelResult<AdapterMetadata> {
    if selector.is_full_ref() {
        let adapter_ref = AdapterRef::parse(selector.as_str())
            .map_err(|err| adapter_store_error(err.to_string()))?;
        let path = layout.adapter_metadata_path(&adapter_ref);
        if path.exists() {
            return store.load_adapter_metadata(layout, &adapter_ref);
        }
    }

    let mut matches = Vec::new();
    if !layout.store_dir.exists() {
        return Err(not_found(selector));
    }

    for entry in fs::read_dir(&layout.store_dir)
        .map_err(|err| path_error("read adapter store directory", &layout.store_dir, err))?
    {
        let entry = entry.map_err(|err| {
            adapter_store_error(format!(
                "read entry in adapter store `{}` failed: {err}",
                layout.store_dir.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            path_error("read adapter store entry type", entry.path().as_path(), err)
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let entry_ref = entry.file_name().to_string_lossy().into_owned();
        if entry_ref.starts_with(selector.as_str()) {
            let adapter_ref = AdapterRef::parse(entry_ref.as_str())
                .map_err(|err| adapter_store_error(err.to_string()))?;
            matches.push(store.load_adapter_metadata(layout, &adapter_ref)?);
        }
    }

    match matches.len() {
        0 => Err(not_found(selector)),
        1 => Ok(matches.remove(0)),
        _ => Err(adapter_store_error(format!(
            "adapter reference `{}` is ambiguous; multiple stored adapters share that prefix",
            selector.as_str()
        ))),
    }
}

fn not_found(selector: &AdapterRefSelector) -> KernelError {
    adapter_store_error(format!(
        "adapter reference `{}` was not found",
        selector.as_str()
    ))
}
