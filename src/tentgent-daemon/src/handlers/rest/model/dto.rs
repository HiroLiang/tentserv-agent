use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::model::domain::{
    ModelInspection, ModelMetadata, ModelRemovalOutcome, ModelSummary,
};

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelItem>,
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub model: ModelItem,
}

#[derive(Debug, Serialize)]
pub struct ModelItem {
    pub model_ref: String,
    pub short_ref: String,
    pub store_path: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
    pub format: String,
    pub detected_formats: Vec<String>,
    pub model_capabilities: Vec<String>,
    pub model_capability_source: String,
    pub source_kind: String,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant_source_path: Option<String>,
}

pub fn model_summary_item(summary: ModelSummary) -> ModelItem {
    model_item_from_parts(summary.metadata, &summary.store_path, None, None)
}

pub fn model_inspection_item(inspection: ModelInspection) -> ModelItem {
    model_item_from_parts(
        inspection.metadata,
        &inspection.store_path,
        Some(&inspection.manifest_path),
        Some(&inspection.variant_source_path),
    )
}

pub fn model_removal_item(outcome: ModelRemovalOutcome) -> ModelItem {
    model_item_from_parts(outcome.metadata, &outcome.store_path, None, None)
}

fn model_item_from_parts(
    metadata: ModelMetadata,
    store_path: &Path,
    manifest_path: Option<&Path>,
    variant_source_path: Option<&Path>,
) -> ModelItem {
    ModelItem {
        model_ref: metadata.model_ref.into_string(),
        short_ref: metadata.short_ref,
        store_path: path_string(store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.primary_format.to_string(),
        detected_formats: metadata
            .detected_formats
            .into_iter()
            .map(|format| format.to_string())
            .collect(),
        model_capabilities: metadata
            .model_capabilities
            .into_iter()
            .map(|capability| capability.to_string())
            .collect(),
        model_capability_source: metadata.model_capability_source.to_string(),
        source_kind: metadata.source_kind.to_string(),
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        source_path: metadata.source_path,
        manifest_path: manifest_path.map(path_string),
        variant_source_path: variant_source_path.map(path_string),
    }
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
