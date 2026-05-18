use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::adapter::domain::{
    AdapterInspection, AdapterMetadata, AdapterRemovalOutcome, AdapterSummary,
};

#[derive(Debug, Serialize)]
pub struct AdaptersResponse {
    pub adapters: Vec<AdapterItem>,
}

#[derive(Debug, Serialize)]
pub struct AdapterResponse {
    pub adapter: AdapterItem,
}

#[derive(Debug, Serialize)]
pub struct AdapterItem {
    pub adapter_ref: String,
    pub short_ref: String,
    pub store_path: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub imported_at: String,
    pub format: String,
    #[serde(rename = "type")]
    pub adapter_type: String,
    pub base_model_ref: Option<String>,
    pub base_model_source_repo: Option<String>,
    pub base_model_source_revision: Option<String>,
    pub model_family: Option<String>,
    pub backend_support: Vec<String>,
    pub source_kind: String,
    pub source_repo: Option<String>,
    pub source_revision: Option<String>,
    pub source_path: Option<String>,
    pub training_dataset_ref: Option<String>,
    pub training_run_ref: Option<String>,
    pub training_config_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_source_path: Option<String>,
}

pub fn adapter_summary_item(summary: AdapterSummary) -> AdapterItem {
    adapter_item_from_parts(summary.metadata, &summary.store_path, None, None)
}

pub fn adapter_inspection_item(inspection: AdapterInspection) -> AdapterItem {
    adapter_item_from_parts(
        inspection.metadata,
        &inspection.store_path,
        Some(&inspection.manifest_path),
        Some(&inspection.source_path),
    )
}

pub fn adapter_removal_item(outcome: AdapterRemovalOutcome) -> AdapterItem {
    adapter_item_from_parts(outcome.metadata, &outcome.store_path, None, None)
}

fn adapter_item_from_parts(
    metadata: AdapterMetadata,
    store_path: &Path,
    manifest_path: Option<&Path>,
    managed_source_path: Option<&Path>,
) -> AdapterItem {
    AdapterItem {
        adapter_ref: metadata.adapter_ref.into_string(),
        short_ref: metadata.short_ref,
        store_path: path_string(store_path),
        file_count: metadata.file_count,
        total_bytes: metadata.total_bytes,
        imported_at: metadata.imported_at,
        format: metadata.adapter_format.to_string(),
        adapter_type: metadata.adapter_type.to_string(),
        base_model_ref: metadata
            .base_model_ref
            .map(|model_ref| model_ref.into_string()),
        base_model_source_repo: metadata.base_model_source_repo,
        base_model_source_revision: metadata.base_model_source_revision,
        model_family: metadata.model_family,
        backend_support: metadata
            .backend_support
            .into_iter()
            .map(|backend| backend.to_string())
            .collect(),
        source_kind: metadata.source_kind.to_string(),
        source_repo: metadata.source_repo,
        source_revision: metadata.source_revision,
        source_path: metadata.source_path,
        training_dataset_ref: metadata.training_dataset_ref,
        training_run_ref: metadata.training_run_ref,
        training_config_ref: metadata.training_config_ref,
        manifest_path: manifest_path.map(path_string),
        managed_source_path: managed_source_path.map(path_string),
    }
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
