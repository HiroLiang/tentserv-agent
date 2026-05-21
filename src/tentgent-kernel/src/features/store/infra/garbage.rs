use std::fs;
use std::path::Path;

use crate::features::adapter::domain::AdapterStoreLayout;
use crate::features::dataset::domain::DatasetStoreLayout;
use crate::features::model::domain::ModelStoreLayout;
use crate::features::store::domain::{ManagedStoreKind, StoreStagingGarbageItem};
use crate::features::store::ports::StoreGarbageCollector;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

#[derive(Debug, Clone, Copy, Default)]
pub struct FileStoreGarbageCollector;

impl StoreGarbageCollector for FileStoreGarbageCollector {
    fn collect_staging_garbage(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<Vec<StoreStagingGarbageItem>> {
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir.clone());
        let adapter_store = AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone());
        let dataset_store = DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone());
        let roots = [
            (ManagedStoreKind::Models, model_store.staging_dir),
            (ManagedStoreKind::Adapters, adapter_store.staging_dir),
            (ManagedStoreKind::Datasets, dataset_store.staging_dir),
        ];
        let mut items = Vec::new();

        for (store, staging_dir) in roots {
            if !staging_dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&staging_dir).map_err(|err| {
                store_gc_error(format!("read `{}` failed: {err}", staging_dir.display()))
            })? {
                let entry = entry.map_err(|err| {
                    store_gc_error(format!(
                        "read `{}` entry failed: {err}",
                        staging_dir.display()
                    ))
                })?;
                let file_type = entry.file_type().map_err(|err| {
                    store_gc_error(format!(
                        "read `{}` file type failed: {err}",
                        entry.path().display()
                    ))
                })?;
                if !file_type.is_dir() {
                    continue;
                }
                let path = entry.path();
                let bytes = dir_size(&path)?;
                items.push(StoreStagingGarbageItem { store, path, bytes });
            }
        }

        items.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(items)
    }

    fn remove_staging_garbage(&self, items: &[StoreStagingGarbageItem]) -> KernelResult<()> {
        for item in items {
            if item.path.exists() {
                fs::remove_dir_all(&item.path).map_err(|err| {
                    store_gc_error(format!("remove `{}` failed: {err}", item.path.display()))
                })?;
            }
        }
        Ok(())
    }
}

fn dir_size(path: &Path) -> KernelResult<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path)
        .map_err(|err| store_gc_error(format!("read `{}` failed: {err}", path.display())))?
    {
        let entry = entry.map_err(|err| {
            store_gc_error(format!("read `{}` entry failed: {err}", path.display()))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            store_gc_error(format!(
                "read `{}` file type failed: {err}",
                entry.path().display()
            ))
        })?;
        if file_type.is_dir() {
            total += dir_size(&entry.path())?;
        } else if file_type.is_file() {
            total += entry
                .metadata()
                .map_err(|err| {
                    store_gc_error(format!(
                        "read `{}` metadata failed: {err}",
                        entry.path().display()
                    ))
                })?
                .len();
        }
    }
    Ok(total)
}

fn store_gc_error(message: impl Into<String>) -> KernelError {
    KernelError::RuntimeStateUnavailable(format!(
        "store garbage collection failed: {}",
        message.into()
    ))
}
