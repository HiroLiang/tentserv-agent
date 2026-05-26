use std::path::Path;

use serde::Serialize;
use tentgent_kernel::features::model::domain::{
    ModelCapabilityProof, ModelImportOutcome, ModelInspection, ModelMetadata, ModelRemovalOutcome,
    ModelSummary,
};
use tentgent_kernel::features::model::usecases::ModelCapabilityUpdateResult;

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelItem>,
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub model: ModelItem,
}

#[derive(Debug, Serialize)]
pub struct ModelMutationResponse {
    pub model: ModelItem,
    pub mutation: ModelMutationItem,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ModelCapabilityUpdateResponse {
    pub model: ModelItem,
    pub mutation: ModelCapabilityUpdateMutationItem,
}

#[derive(Debug, Serialize)]
pub struct ModelCapabilityProofsResponse {
    pub model: ModelItem,
    pub proofs: Vec<ModelCapabilityProofItem>,
}

#[derive(Debug, Serialize)]
pub struct ModelCapabilityVerifyResponse {
    pub model: ModelItem,
    pub proof: ModelCapabilityProofItem,
}

#[derive(Debug, Serialize)]
pub struct ModelCapabilityUpdateMutationItem {
    pub kind: &'static str,
    pub previous_capabilities: Vec<String>,
    pub capabilities: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ModelMutationItem {
    pub kind: &'static str,
    pub deduplicated: bool,
    pub store_path: String,
    pub source_index_path: String,
}

#[derive(Debug, Serialize)]
pub struct ModelCapabilityProofItem {
    pub model_ref: String,
    pub capability: String,
    pub status: String,
    pub source: String,
    pub primary_format: String,
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mlx_runtime_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_ref: Option<String>,
    pub checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mlx_runtime_family: Option<String>,
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

pub fn model_mutation_response(
    outcome: ModelImportOutcome,
    kind: &'static str,
) -> ModelMutationResponse {
    let warnings = outcome
        .metadata
        .capability_warning()
        .map(str::to_string)
        .into_iter()
        .collect();
    let mutation = ModelMutationItem {
        kind,
        deduplicated: outcome.deduplicated,
        store_path: path_string(&outcome.store_path),
        source_index_path: path_string(&outcome.source_index_path),
    };
    ModelMutationResponse {
        model: model_item_from_parts(outcome.metadata, &outcome.store_path, None, None),
        mutation,
        warnings,
    }
}

pub fn model_capability_update_response(
    result: ModelCapabilityUpdateResult,
) -> ModelCapabilityUpdateResponse {
    let capabilities = result
        .model
        .metadata
        .model_capabilities
        .iter()
        .map(|capability| capability.to_string())
        .collect();
    let previous_capabilities = result
        .previous_capabilities
        .into_iter()
        .map(|capability| capability.to_string())
        .collect();
    let added = result
        .added_capabilities
        .into_iter()
        .map(|capability| capability.to_string())
        .collect();
    let removed = result
        .removed_capabilities
        .into_iter()
        .map(|capability| capability.to_string())
        .collect();

    ModelCapabilityUpdateResponse {
        model: model_item_from_parts(
            result.model.metadata,
            &result.model.store_path,
            Some(&result.model.manifest_path),
            Some(&result.model.variant_source_path),
        ),
        mutation: ModelCapabilityUpdateMutationItem {
            kind: "update_capabilities",
            previous_capabilities,
            capabilities,
            added,
            removed,
        },
    }
}

pub fn model_capability_proofs_response(
    inspection: ModelInspection,
    proofs: Vec<ModelCapabilityProof>,
) -> ModelCapabilityProofsResponse {
    ModelCapabilityProofsResponse {
        model: model_item_from_parts(
            inspection.metadata,
            &inspection.store_path,
            Some(&inspection.manifest_path),
            Some(&inspection.variant_source_path),
        ),
        proofs: proofs
            .into_iter()
            .map(model_capability_proof_item)
            .collect(),
    }
}

pub fn model_capability_verify_response(
    inspection: ModelInspection,
    proof: ModelCapabilityProof,
) -> ModelCapabilityVerifyResponse {
    ModelCapabilityVerifyResponse {
        model: model_item_from_parts(
            inspection.metadata,
            &inspection.store_path,
            Some(&inspection.manifest_path),
            Some(&inspection.variant_source_path),
        ),
        proof: model_capability_proof_item(proof),
    }
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
        mlx_runtime_family: metadata.mlx_runtime_family.map(|family| family.to_string()),
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

fn model_capability_proof_item(proof: ModelCapabilityProof) -> ModelCapabilityProofItem {
    ModelCapabilityProofItem {
        model_ref: proof.model_ref.into_string(),
        capability: proof.capability.to_string(),
        status: proof.status.to_string(),
        source: proof.source.to_string(),
        primary_format: proof.primary_format.to_string(),
        backend: proof.backend,
        mlx_runtime_family: proof.mlx_runtime_family.map(|family| family.to_string()),
        runtime_version: proof.runtime_version,
        server_ref: proof.server_ref,
        checked_at: proof.checked_at,
        error: proof.error,
    }
}

fn path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().display().to_string()
}
