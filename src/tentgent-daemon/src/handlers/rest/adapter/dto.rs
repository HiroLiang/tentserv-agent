use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::adapter::domain::{
    AdapterBindOutcome, AdapterImportOutcome, AdapterInspection, AdapterMetadata,
    AdapterRemovalOutcome, AdapterSummary,
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
pub struct AdapterMutationResponse {
    pub adapter: AdapterItem,
    pub mutation: AdapterMutationItem,
}

#[derive(Debug, Serialize)]
pub struct AdapterMutationItem {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicated: Option<bool>,
    pub store_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_index_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_index_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed_base_index_path: Option<String>,
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
    pub target_capability: Option<String>,
    pub base_model_ref: Option<String>,
    pub base_model_source_repo: Option<String>,
    pub base_model_source_revision: Option<String>,
    pub model_family: Option<String>,
    pub backend_support: Vec<String>,
    pub control_kind: Option<String>,
    pub weight_file: Option<String>,
    pub trigger_words: Vec<String>,
    pub recommended_scale: Option<f32>,
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

pub fn adapter_import_mutation_response(
    outcome: AdapterImportOutcome,
    kind: &'static str,
) -> AdapterMutationResponse {
    let mutation = AdapterMutationItem {
        kind,
        deduplicated: Some(outcome.deduplicated),
        store_path: path_string(&outcome.store_path),
        base_model_ref: outcome
            .metadata
            .base_model_ref
            .as_ref()
            .map(ToString::to_string),
        source_index_path: Some(path_string(&outcome.source_index_path)),
        base_index_path: outcome.base_index_path.as_ref().map(path_string),
        removed_base_index_path: None,
    };
    AdapterMutationResponse {
        adapter: adapter_item_from_parts(outcome.metadata, &outcome.store_path, None, None),
        mutation,
    }
}

pub fn adapter_bind_response(outcome: AdapterBindOutcome) -> AdapterMutationResponse {
    let mutation = AdapterMutationItem {
        kind: "bind",
        deduplicated: None,
        store_path: path_string(&outcome.store_path),
        base_model_ref: outcome
            .metadata
            .base_model_ref
            .as_ref()
            .map(ToString::to_string),
        source_index_path: None,
        base_index_path: Some(path_string(&outcome.base_index_path)),
        removed_base_index_path: outcome.removed_base_index_path.as_ref().map(path_string),
    };
    AdapterMutationResponse {
        adapter: adapter_item_from_parts(outcome.metadata, &outcome.store_path, None, None),
        mutation,
    }
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
        target_capability: metadata
            .target_capability
            .map(|capability| capability.to_string()),
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
        control_kind: metadata.control_kind,
        weight_file: metadata.weight_file,
        trigger_words: metadata.trigger_words,
        recommended_scale: metadata.recommended_scale.map(|scale| scale.as_f32()),
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
