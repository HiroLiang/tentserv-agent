use std::path::PathBuf;

use crate::features::model::domain::{ModelCapability, ModelRef};

use super::domain::{
    backend_support_for_format, detect_adapter_format, escape_huggingface_repo_id,
    validate_adapter_compatibility, AdapterBackendSupport, AdapterCompatibilityError,
    AdapterCompatibilityTarget, AdapterFormat, AdapterManifest, AdapterManifestEntry,
    AdapterMetadata, AdapterRef, AdapterRefParseError, AdapterRefSelector, AdapterSourceKind,
    AdapterStoreLayout, AdapterType, ADAPTER_MANIFEST_FILENAME, ADAPTER_METADATA_FILENAME,
    HUGGINGFACE_SOURCE_DIRNAME, LOCAL_SOURCE_DIRNAME, MLX_ADAPTERS_FILENAME,
    PEFT_ADAPTER_MODEL_FILENAME, SHORT_ADAPTER_REF_LENGTH, SOURCE_DIRNAME, STAGING_DIRNAME,
    STORE_DIRNAME, TRAIN_RUN_SOURCE_DIRNAME,
};

#[test]
fn adapter_ref_is_canonical_sha256_hex_and_derives_short_ref() {
    let uppercase = "A".repeat(64);
    let adapter_ref = AdapterRef::parse(&uppercase).expect("adapter ref");

    assert_eq!(adapter_ref.as_str(), "a".repeat(64));
    assert_eq!(
        adapter_ref.short_ref(),
        "a".repeat(SHORT_ADAPTER_REF_LENGTH)
    );
}

#[test]
fn adapter_ref_selector_accepts_short_or_full_hex_prefixes() {
    let short = AdapterRefSelector::parse("ABC123").expect("short selector");
    assert_eq!(short.as_str(), "abc123");
    assert!(!short.is_full_ref());

    let full = AdapterRefSelector::parse("b".repeat(64)).expect("full selector");
    assert!(full.is_full_ref());
}

#[test]
fn adapter_ref_validation_rejects_empty_wrong_length_and_non_hex_values() {
    assert_eq!(
        AdapterRef::parse(""),
        Err(AdapterRefParseError::Empty),
        "empty refs should not enter adapter store logic"
    );
    assert_eq!(
        AdapterRef::parse("abc"),
        Err(AdapterRefParseError::InvalidFullLength { actual: 3 })
    );
    assert_eq!(
        AdapterRefSelector::parse("../abc"),
        Err(AdapterRefParseError::NonHex)
    );
}

#[test]
fn adapter_store_layout_matches_contract_paths() {
    let layout = AdapterStoreLayout::from_adapters_dir("/tmp/tentgent/adapters");
    let adapter_ref = AdapterRef::parse("1".repeat(64)).expect("adapter ref");
    let base_model_ref = ModelRef::parse("2".repeat(64)).expect("model ref");

    assert_eq!(
        layout.store_dir,
        PathBuf::from("/tmp/tentgent/adapters").join(STORE_DIRNAME)
    );
    assert_eq!(
        layout.hf_index_dir,
        PathBuf::from("/tmp/tentgent/adapters")
            .join("by-source")
            .join(HUGGINGFACE_SOURCE_DIRNAME)
    );
    assert_eq!(
        layout.local_index_dir,
        PathBuf::from("/tmp/tentgent/adapters")
            .join("by-source")
            .join(LOCAL_SOURCE_DIRNAME)
    );
    assert_eq!(
        layout.train_run_index_dir,
        PathBuf::from("/tmp/tentgent/adapters")
            .join("by-source")
            .join(TRAIN_RUN_SOURCE_DIRNAME)
    );
    assert_eq!(
        layout.staging_dir,
        PathBuf::from("/tmp/tentgent/adapters").join(STAGING_DIRNAME)
    );
    assert_eq!(
        layout.adapter_metadata_path(&adapter_ref),
        PathBuf::from("/tmp/tentgent/adapters")
            .join(STORE_DIRNAME)
            .join(adapter_ref.as_str())
            .join(ADAPTER_METADATA_FILENAME)
    );
    assert_eq!(
        layout.manifest_path(&adapter_ref),
        PathBuf::from("/tmp/tentgent/adapters")
            .join(STORE_DIRNAME)
            .join(adapter_ref.as_str())
            .join(ADAPTER_MANIFEST_FILENAME)
    );
    assert_eq!(
        layout.source_dir(&adapter_ref),
        PathBuf::from("/tmp/tentgent/adapters")
            .join(STORE_DIRNAME)
            .join(adapter_ref.as_str())
            .join(SOURCE_DIRNAME)
    );
    assert_eq!(
        layout.base_index_path(&base_model_ref, &adapter_ref),
        PathBuf::from("/tmp/tentgent/adapters")
            .join("by-base")
            .join(base_model_ref.as_str())
            .join(format!("{}.toml", adapter_ref.as_str()))
    );
}

#[test]
fn adapter_format_detection_maps_known_files_to_backend_support() {
    let peft = AdapterManifest {
        files: vec![manifest_entry(PEFT_ADAPTER_MODEL_FILENAME, 8)],
    };
    let mlx = AdapterManifest {
        files: vec![manifest_entry(MLX_ADAPTERS_FILENAME, 8)],
    };

    assert_eq!(detect_adapter_format(&peft), Ok(AdapterFormat::Peft));
    assert_eq!(
        backend_support_for_format(AdapterFormat::Peft),
        vec![AdapterBackendSupport::TransformersPeft]
    );
    assert_eq!(detect_adapter_format(&mlx), Ok(AdapterFormat::Mlx));
    assert_eq!(
        backend_support_for_format(AdapterFormat::Mlx),
        vec![AdapterBackendSupport::Mlx]
    );
}

#[test]
fn adapter_manifest_sorts_counts_and_sums_files_without_io() {
    let manifest = AdapterManifest {
        files: vec![
            manifest_entry("z/adapter_config.json", 2),
            manifest_entry(PEFT_ADAPTER_MODEL_FILENAME, 3),
        ],
    }
    .sorted();

    assert_eq!(manifest.files[0].relative_path, PEFT_ADAPTER_MODEL_FILENAME);
    assert_eq!(manifest.file_count(), 2);
    assert_eq!(manifest.total_bytes(), 5);
    assert!(manifest.contains_path(PEFT_ADAPTER_MODEL_FILENAME));
    assert!(!manifest.is_empty());
}

#[test]
fn adapter_metadata_reports_source_base_and_short_ref_consistency() {
    let adapter_ref = AdapterRef::parse("c".repeat(64)).expect("adapter ref");
    let metadata = AdapterMetadata {
        adapter_ref,
        short_ref: "c".repeat(SHORT_ADAPTER_REF_LENGTH),
        adapter_format: AdapterFormat::Peft,
        adapter_type: AdapterType::Lora,
        base_model_ref: None,
        base_model_source_repo: Some("org/base".to_string()),
        base_model_source_revision: Some("base-sha".to_string()),
        model_family: Some("llama".to_string()),
        backend_support: vec![AdapterBackendSupport::TransformersPeft],
        source_kind: AdapterSourceKind::HuggingFace,
        source_repo: Some("org/adapter".to_string()),
        source_revision: Some("adapter-sha".to_string()),
        source_path: None,
        training_dataset_ref: None,
        training_run_ref: None,
        training_config_ref: None,
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    };

    assert!(metadata.has_consistent_short_ref());
    assert_eq!(metadata.source_summary(), "org/adapter@adapter-sha");
    assert_eq!(metadata.base_model_summary(), "org/base@base-sha");
}

#[test]
fn adapter_compatibility_requires_backend_and_base_model_proof() {
    let base_model_ref = ModelRef::parse("b".repeat(64)).expect("model ref");
    let metadata = metadata_fixture(None);
    let target = AdapterCompatibilityTarget {
        base_model_ref,
        base_model_source_repo: Some("org/base".to_string()),
        base_model_source_revision: Some("base-sha".to_string()),
        base_model_capabilities: vec![ModelCapability::Chat],
        backend: AdapterBackendSupport::TransformersPeft,
    };

    assert_eq!(validate_adapter_compatibility(&metadata, &target), Ok(()));

    let unsupported_backend = AdapterCompatibilityTarget {
        backend: AdapterBackendSupport::Mlx,
        ..target.clone()
    };
    assert_eq!(
        validate_adapter_compatibility(&metadata, &unsupported_backend),
        Err(AdapterCompatibilityError::UnsupportedBackend {
            backend: "mlx".to_string()
        })
    );

    let missing_proof = AdapterCompatibilityTarget {
        base_model_source_repo: None,
        ..target
    };
    assert_eq!(
        validate_adapter_compatibility(&metadata, &missing_proof),
        Err(AdapterCompatibilityError::MissingBaseModelProof)
    );
}

#[test]
fn exact_base_model_ref_compatibility_wins_over_source_hints() {
    let base_model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    let metadata = metadata_fixture(Some(base_model_ref.clone()));
    let target = AdapterCompatibilityTarget {
        base_model_ref,
        base_model_source_repo: Some("different/base".to_string()),
        base_model_source_revision: Some("different-sha".to_string()),
        base_model_capabilities: vec![ModelCapability::Chat],
        backend: AdapterBackendSupport::TransformersPeft,
    };

    assert_eq!(validate_adapter_compatibility(&metadata, &target), Ok(()));
}

#[test]
fn adapter_compatibility_rejects_non_chat_base_models() {
    let base_model_ref = ModelRef::parse("e".repeat(64)).expect("model ref");
    let metadata = metadata_fixture(Some(base_model_ref.clone()));
    let target = AdapterCompatibilityTarget {
        base_model_ref,
        base_model_source_repo: Some("org/base".to_string()),
        base_model_source_revision: Some("base-sha".to_string()),
        base_model_capabilities: vec![ModelCapability::Embedding],
        backend: AdapterBackendSupport::TransformersPeft,
    };

    assert_eq!(
        validate_adapter_compatibility(&metadata, &target),
        Err(AdapterCompatibilityError::UnsupportedBaseModelCapability {
            required: "chat".to_string()
        })
    );
}

#[test]
fn huggingface_repo_escape_matches_source_index_contract() {
    assert_eq!(
        escape_huggingface_repo_id("org/adapter-name"),
        "org--adapter-name"
    );
}

fn metadata_fixture(base_model_ref: Option<ModelRef>) -> AdapterMetadata {
    AdapterMetadata {
        adapter_ref: AdapterRef::parse("a".repeat(64)).expect("adapter ref"),
        short_ref: "a".repeat(SHORT_ADAPTER_REF_LENGTH),
        adapter_format: AdapterFormat::Peft,
        adapter_type: AdapterType::Lora,
        base_model_ref,
        base_model_source_repo: Some("org/base".to_string()),
        base_model_source_revision: Some("base-sha".to_string()),
        model_family: Some("llama".to_string()),
        backend_support: vec![AdapterBackendSupport::TransformersPeft],
        source_kind: AdapterSourceKind::Local,
        source_repo: None,
        source_revision: None,
        source_path: Some("/tmp/adapter".to_string()),
        training_dataset_ref: None,
        training_run_ref: None,
        training_config_ref: None,
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    }
}

fn manifest_entry(relative_path: &str, size_bytes: u64) -> AdapterManifestEntry {
    AdapterManifestEntry {
        relative_path: relative_path.to_string(),
        size_bytes,
        sha256: "0".repeat(64),
    }
}
