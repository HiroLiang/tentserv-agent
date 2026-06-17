use super::*;
use crate::features::model::domain::{
    default_model_capability_source, MlxRuntimeFamily, ModelCapability, ModelFormat, ModelMetadata,
    ModelRef, ModelSourceKind,
};
use crate::features::model::support_status::{ModelSupportQuery, ModelSupportStatusResolver};

#[test]
fn built_in_catalog_parses_many_major_models() {
    let catalog = built_in_model_support_catalog().expect("parse built-in catalog");
    let known_source_refs = catalog
        .models
        .iter()
        .map(|entry| entry.source_repos.len() + entry.source_repo_patterns.len())
        .sum::<usize>();

    assert_eq!(catalog.schema_version, 1);
    assert!(
        known_source_refs >= 100,
        "catalog should cover many major model refs and patterns, got {known_source_refs}"
    );
}

#[test]
fn exact_fixture_entry_produces_supported_hint() {
    let metadata = hf_metadata(
        "Qwen/Qwen2.5-0.5B-Instruct",
        ModelFormat::Safetensors,
        None,
        vec![ModelCapability::Chat],
    );

    let entries = built_in_catalog_entries_for_model(&metadata);
    let hints = built_in_support_hints_for_model(&metadata);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].support_level,
        ModelSupportCatalogLevel::FixtureSupported
    );
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].capability, ModelCapability::Chat);
}

#[test]
fn large_external_model_is_known_without_supported_hint() {
    let metadata = hf_metadata(
        "nvidia/nemotron-3-ultra-550b-a55b",
        ModelFormat::Safetensors,
        None,
        vec![ModelCapability::Chat],
    );

    let entries = built_in_catalog_entries_for_model(&metadata);
    let hints = built_in_support_hints_for_model(&metadata);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].support_level,
        ModelSupportCatalogLevel::RequiresExternalRuntime
    );
    assert!(hints.is_empty());
}

#[test]
fn mlx_pattern_entry_matches_family_conversion() {
    let metadata = hf_metadata(
        "mlx-community/Qwen3-8B-4bit",
        ModelFormat::Mlx,
        Some(MlxRuntimeFamily::Lm),
        vec![ModelCapability::Chat],
    );

    let entries = built_in_catalog_entries_for_model(&metadata);
    let hints = built_in_support_hints_for_model(&metadata);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].publisher, "mlx-community");
    assert_eq!(
        entries[0].support_level,
        ModelSupportCatalogLevel::LocalRuntimeSupported
    );
    assert_eq!(hints.len(), 1);
}

#[test]
fn non_catalog_model_returns_no_entries_or_hints() {
    let metadata = hf_metadata(
        "unknown/example-model",
        ModelFormat::Safetensors,
        None,
        vec![ModelCapability::Chat],
    );

    assert!(built_in_catalog_entries_for_model(&metadata).is_empty());
    assert!(built_in_support_hints_for_model(&metadata).is_empty());
}

#[test]
fn local_proof_still_beats_catalog_hint() {
    let metadata = hf_metadata(
        "Qwen/Qwen2.5-0.5B-Instruct",
        ModelFormat::Safetensors,
        None,
        vec![ModelCapability::Chat],
    );
    let query = ModelSupportQuery::from_metadata(&metadata, ModelCapability::Chat);
    let hints = built_in_support_hints_for_model(&metadata);
    let proof = crate::features::model::domain::ModelCapabilityProof {
        model_ref: metadata.model_ref.clone(),
        capability: ModelCapability::Chat,
        status: crate::features::model::domain::ModelCapabilityProofStatus::Failed,
        source: crate::features::model::domain::ModelCapabilityProofSource::EndpointSmoke,
        primary_format: ModelFormat::Safetensors,
        mlx_runtime_family: None,
        backend: "safetensors".to_string(),
        runtime_version: None,
        runtime_profile: None,
        runtime_profile_version: None,
        server_ref: None,
        checked_at: "2026-06-12T00:00:00Z".to_string(),
        error: Some("fixture failed locally".to_string()),
    };

    let resolution = ModelSupportStatusResolver.resolve(&metadata, &query, &[proof], &hints);

    assert_eq!(
        resolution.status,
        crate::features::model::support_status::ModelSupportStatus::Failed
    );
    assert_eq!(
        resolution.failure_reason.as_deref(),
        Some("fixture failed locally")
    );
}

fn hf_metadata(
    source_repo: &str,
    primary_format: ModelFormat,
    mlx_runtime_family: Option<MlxRuntimeFamily>,
    capabilities: Vec<ModelCapability>,
) -> ModelMetadata {
    let model_ref = ModelRef::parse("b".repeat(64)).expect("model ref");
    ModelMetadata {
        short_ref: model_ref.short_ref().to_string(),
        model_ref,
        source_kind: ModelSourceKind::HuggingFace,
        source_repo: Some(source_repo.to_string()),
        source_revision: Some("main".to_string()),
        source_path: None,
        primary_format,
        detected_formats: vec![primary_format],
        mlx_runtime_family,
        model_capabilities: capabilities,
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-06-12T00:00:00Z".to_string(),
    }
}
