//! Built-in model support catalog loader and hint conversion.

use super::super::domain::{MlxRuntimeFamily, ModelFormat, ModelMetadata};
use super::super::support_status::ModelSupportHint;
use super::domain::{
    ModelCatalogHintStatus, ModelSupportCatalogDocument, ModelSupportCatalogEntry,
};
use super::matching::{entry_match_kind, ModelCatalogMatchKind};

const BUILT_IN_MODEL_SUPPORT_CATALOG: &str = include_str!("built_in.toml");

pub fn built_in_model_support_catalog() -> Result<ModelSupportCatalogDocument, toml::de::Error> {
    toml::from_str(BUILT_IN_MODEL_SUPPORT_CATALOG)
}

pub fn built_in_catalog_entries_for_model(
    metadata: &ModelMetadata,
) -> Vec<ModelSupportCatalogEntry> {
    let Ok(document) = built_in_model_support_catalog() else {
        return Vec::new();
    };

    let mut exact = Vec::new();
    let mut pattern = Vec::new();

    for entry in document.models {
        match entry_match_kind(&entry, metadata) {
            ModelCatalogMatchKind::Exact => exact.push(entry),
            ModelCatalogMatchKind::Pattern => pattern.push(entry),
            ModelCatalogMatchKind::None => {}
        }
    }

    if exact.is_empty() {
        pattern
    } else {
        exact
    }
}

pub fn built_in_support_hints_for_model(metadata: &ModelMetadata) -> Vec<ModelSupportHint> {
    built_in_catalog_entries_for_model(metadata)
        .into_iter()
        .flat_map(|entry| support_hints_for_entry(&entry))
        .collect()
}

fn support_hints_for_entry(entry: &ModelSupportCatalogEntry) -> Vec<ModelSupportHint> {
    match entry.support_level.hint_status() {
        Some(ModelCatalogHintStatus::Supported) => entry
            .capabilities
            .iter()
            .copied()
            .map(|capability| {
                ModelSupportHint::supported(capability, entry.support_hint_reason())
                    .with_optional_primary_format(entry.primary_formats.first().copied())
                    .with_optional_mlx_runtime_family(entry.mlx_runtime_families.first().copied())
                    .with_optional_backend(entry.backends.first().cloned())
            })
            .collect(),
        Some(ModelCatalogHintStatus::Unsupported) => entry
            .capabilities
            .iter()
            .copied()
            .map(|capability| {
                ModelSupportHint::unsupported(capability, entry.support_hint_reason())
                    .with_optional_primary_format(entry.primary_formats.first().copied())
                    .with_optional_mlx_runtime_family(entry.mlx_runtime_families.first().copied())
                    .with_optional_backend(entry.backends.first().cloned())
            })
            .collect(),
        None => Vec::new(),
    }
}

trait OptionalModelSupportHintFields {
    fn with_optional_primary_format(self, primary_format: Option<ModelFormat>) -> Self;
    fn with_optional_mlx_runtime_family(self, mlx_runtime_family: Option<MlxRuntimeFamily>)
        -> Self;
    fn with_optional_backend(self, backend: Option<String>) -> Self;
}

impl OptionalModelSupportHintFields for ModelSupportHint {
    fn with_optional_primary_format(self, primary_format: Option<ModelFormat>) -> Self {
        match primary_format {
            Some(primary_format) => self.with_primary_format(primary_format),
            None => self,
        }
    }

    fn with_optional_mlx_runtime_family(
        self,
        mlx_runtime_family: Option<MlxRuntimeFamily>,
    ) -> Self {
        match mlx_runtime_family {
            Some(mlx_runtime_family) => self.with_mlx_runtime_family(mlx_runtime_family),
            None => self,
        }
    }

    fn with_optional_backend(self, backend: Option<String>) -> Self {
        match backend {
            Some(backend) => self.with_backend(backend),
            None => self,
        }
    }
}
