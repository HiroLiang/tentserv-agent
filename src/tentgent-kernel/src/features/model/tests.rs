use std::path::PathBuf;

use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeSource};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::domain::{
    default_model_capabilities, default_model_capability_source, detect_model_formats,
    escape_huggingface_repo_id, infer_mlx_runtime_family, select_primary_model_format,
    HfModelPullProgress, HfModelSourceIndex, LocalModelSourceIndex, MlxRuntimeFamily,
    ModelCapability, ModelCapabilitySource, ModelFormat, ModelImportMethod, ModelManifest,
    ModelManifestEntry, ModelMetadata, ModelRef, ModelRefParseError, ModelRefSelector,
    ModelSourceKind, ModelStoreLayout, ModelVariantMetadata, ModelVariantStatus,
    HUGGINGFACE_SOURCE_DIRNAME, LOCAL_SOURCE_DIRNAME, MODEL_MANIFEST_FILENAME,
    MODEL_METADATA_FILENAME, SHORT_MODEL_REF_LENGTH, SOURCE_DIRNAME, STAGING_DIRNAME,
    STORE_DIRNAME, VARIANTS_DIRNAME, VARIANT_METADATA_FILENAME,
};
use super::ports::{
    HfModelSnapshot, HfModelSnapshotFetcher, HfModelSnapshotRequest, ModelCatalogStore,
    ModelContentStore, ModelIdentityGenerator, ModelManifestBuilder, ModelServerReferenceProbe,
    ModelSourceIndexStore, ModelSourceStager, ModelStoreLayoutInitializer, StagedModelSource,
};

#[test]
fn model_ref_is_canonical_sha256_hex_and_derives_short_ref() {
    let uppercase = "A".repeat(64);
    let model_ref = ModelRef::parse(&uppercase).expect("model ref");

    assert_eq!(model_ref.as_str(), "a".repeat(64));
    assert_eq!(model_ref.short_ref(), "a".repeat(SHORT_MODEL_REF_LENGTH));
}

#[test]
fn model_ref_selector_accepts_short_or_full_hex_prefixes() {
    let short = ModelRefSelector::parse("ABC123").expect("short selector");
    assert_eq!(short.as_str(), "abc123");
    assert!(!short.is_full_ref());

    let full = ModelRefSelector::parse("b".repeat(64)).expect("full selector");
    assert!(full.is_full_ref());
}

#[test]
fn model_ref_validation_rejects_empty_wrong_length_and_non_hex_values() {
    assert_eq!(
        ModelRef::parse(""),
        Err(ModelRefParseError::Empty),
        "empty refs should not enter model store logic"
    );
    assert_eq!(
        ModelRef::parse("abc"),
        Err(ModelRefParseError::InvalidFullLength { actual: 3 })
    );
    assert_eq!(
        ModelRefSelector::parse("../abc"),
        Err(ModelRefParseError::NonHex)
    );
}

#[test]
fn model_store_layout_matches_contract_paths() {
    let layout = ModelStoreLayout::from_models_dir("/tmp/tentgent/models");
    let model_ref = ModelRef::parse("1".repeat(64)).expect("model ref");

    assert_eq!(
        layout.store_dir,
        PathBuf::from("/tmp/tentgent/models").join(STORE_DIRNAME)
    );
    assert_eq!(
        layout.hf_index_dir,
        PathBuf::from("/tmp/tentgent/models")
            .join("by-source")
            .join(HUGGINGFACE_SOURCE_DIRNAME)
    );
    assert_eq!(
        layout.local_index_dir,
        PathBuf::from("/tmp/tentgent/models")
            .join("by-source")
            .join(LOCAL_SOURCE_DIRNAME)
    );
    assert_eq!(
        layout.staging_dir,
        PathBuf::from("/tmp/tentgent/models").join(STAGING_DIRNAME)
    );
    assert_eq!(
        layout.model_metadata_path(&model_ref),
        PathBuf::from("/tmp/tentgent/models")
            .join(STORE_DIRNAME)
            .join(model_ref.as_str())
            .join(MODEL_METADATA_FILENAME)
    );
    assert_eq!(
        layout.manifest_path(&model_ref),
        PathBuf::from("/tmp/tentgent/models")
            .join(STORE_DIRNAME)
            .join(model_ref.as_str())
            .join(MODEL_MANIFEST_FILENAME)
    );
    assert_eq!(
        layout.variant_source_dir(&model_ref, ModelFormat::Safetensors),
        PathBuf::from("/tmp/tentgent/models")
            .join(STORE_DIRNAME)
            .join(model_ref.as_str())
            .join(VARIANTS_DIRNAME)
            .join("safetensors")
            .join(SOURCE_DIRNAME)
    );
    assert_eq!(
        layout.variant_metadata_path(&model_ref, ModelFormat::Safetensors),
        PathBuf::from("/tmp/tentgent/models")
            .join(STORE_DIRNAME)
            .join(model_ref.as_str())
            .join(VARIANTS_DIRNAME)
            .join("safetensors")
            .join(VARIANT_METADATA_FILENAME)
    );
}

#[test]
fn format_detection_keeps_contract_order_and_mlx_repo_rule() {
    let manifest = ModelManifest {
        files: vec![
            manifest_entry("model_index.json", 2),
            manifest_entry("weights/model.gguf", 8),
            manifest_entry("model.safetensors.index.json", 4),
        ],
    };

    let detected = detect_model_formats(&manifest, Some("mlx-community/example"));

    assert_eq!(
        detected,
        vec![
            ModelFormat::Mlx,
            ModelFormat::Diffusers,
            ModelFormat::Safetensors,
            ModelFormat::Gguf
        ]
    );
    assert_eq!(
        select_primary_model_format(&detected, Some("mlx-community/example")),
        Ok(ModelFormat::Mlx)
    );
    assert_eq!(
        select_primary_model_format(&detected, Some("other/example")),
        Ok(ModelFormat::Diffusers)
    );
}

#[test]
fn manifest_sorts_counts_and_sums_files_without_io() {
    let manifest = ModelManifest {
        files: vec![manifest_entry("b.gguf", 2), manifest_entry("a.gguf", 3)],
    }
    .sorted();

    assert_eq!(manifest.files[0].relative_path, "a.gguf");
    assert_eq!(manifest.file_count(), 2);
    assert_eq!(manifest.total_bytes(), 5);
    assert!(!manifest.is_empty());
}

#[test]
fn metadata_reports_source_summary_and_short_ref_consistency() {
    let model_ref = ModelRef::parse("c".repeat(64)).expect("model ref");
    let metadata = ModelMetadata {
        model_ref,
        short_ref: "c".repeat(SHORT_MODEL_REF_LENGTH),
        source_kind: ModelSourceKind::HuggingFace,
        source_repo: Some("org/model".to_string()),
        source_revision: Some("resolved-sha".to_string()),
        source_path: None,
        primary_format: ModelFormat::Gguf,
        detected_formats: vec![ModelFormat::Gguf],
        mlx_runtime_family: None,
        model_capabilities: vec![ModelCapability::Chat, ModelCapability::Embedding],
        model_capability_source: ModelCapabilitySource::ExplicitUser,
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    };

    assert!(metadata.has_consistent_short_ref());
    assert!(metadata.supports_capability(ModelCapability::Chat));
    assert!(metadata.supports_capability(ModelCapability::Embedding));
    assert!(!metadata.supports_capability(ModelCapability::Rerank));
    assert_eq!(
        metadata.model_capability_source,
        ModelCapabilitySource::ExplicitUser
    );
    assert_eq!(metadata.source_summary(), "org/model@resolved-sha");
}

#[test]
fn model_metadata_defaults_missing_capabilities_to_chat() {
    let body = format!(
        r#"{{
            "model_ref": "{model_ref}",
            "short_ref": "{short_ref}",
            "source_kind": "local",
            "source_repo": null,
            "source_revision": null,
            "source_path": "/tmp/source",
            "primary_format": "gguf",
            "detected_formats": ["gguf"],
            "file_count": 1,
            "total_bytes": 42,
            "imported_at": "2026-05-17T00:00:00Z"
        }}"#,
        model_ref = "e".repeat(64),
        short_ref = "e".repeat(SHORT_MODEL_REF_LENGTH)
    );

    let metadata: ModelMetadata = serde_json::from_str(&body).expect("metadata");

    assert_eq!(metadata.model_capabilities, vec![ModelCapability::Chat]);
    assert_eq!(
        metadata.model_capability_source,
        ModelCapabilitySource::DefaultChat
    );
}

#[test]
fn model_metadata_capabilities_round_trip_as_strings() {
    let model_ref = ModelRef::parse("f".repeat(64)).expect("model ref");
    let metadata = ModelMetadata {
        short_ref: model_ref.short_ref().to_string(),
        model_ref,
        source_kind: ModelSourceKind::Local,
        source_repo: None,
        source_revision: None,
        source_path: Some("/tmp/source".to_string()),
        primary_format: ModelFormat::Safetensors,
        detected_formats: vec![ModelFormat::Safetensors],
        mlx_runtime_family: None,
        model_capabilities: vec![
            ModelCapability::Chat,
            ModelCapability::Rerank,
            ModelCapability::AudioTranscription,
        ],
        model_capability_source: ModelCapabilitySource::ManualUpdate,
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    };

    let body = toml::to_string_pretty(&metadata).expect("serialize metadata");
    assert!(body.contains("\"chat\""));
    assert!(body.contains("\"rerank\""));
    assert!(body.contains("\"audio-transcription\""));
    assert!(body.contains("model_capability_source = \"manual-update\""));

    let parsed: ModelMetadata = toml::from_str(&body).expect("parse metadata");
    assert_eq!(
        parsed.model_capabilities,
        vec![
            ModelCapability::Chat,
            ModelCapability::Rerank,
            ModelCapability::AudioTranscription
        ]
    );
    assert_eq!(
        parsed.model_capability_source,
        ModelCapabilitySource::ManualUpdate
    );
}

#[test]
fn model_metadata_defaults_missing_mlx_runtime_family_to_none() {
    let body = format!(
        r#"{{
            "model_ref": "{model_ref}",
            "short_ref": "{short_ref}",
            "source_kind": "huggingface",
            "source_repo": "mlx-community/demo",
            "source_revision": "main",
            "source_path": null,
            "primary_format": "mlx",
            "detected_formats": ["mlx"],
            "model_capabilities": ["chat"],
            "model_capability_source": "explicit-user",
            "file_count": 1,
            "total_bytes": 42,
            "imported_at": "2026-05-17T00:00:00Z"
        }}"#,
        model_ref = "d".repeat(64),
        short_ref = "d".repeat(SHORT_MODEL_REF_LENGTH)
    );

    let metadata: ModelMetadata = serde_json::from_str(&body).expect("metadata");

    assert_eq!(metadata.mlx_runtime_family, None);
}

#[test]
fn mlx_runtime_family_round_trips_as_explicit_backend_family() {
    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    struct FamilyDocument {
        mlx_runtime_family: MlxRuntimeFamily,
    }

    let family = MlxRuntimeFamily::Vlm;
    let body = toml::to_string(&FamilyDocument {
        mlx_runtime_family: family,
    })
    .expect("serialize family");

    assert!(body.contains("mlx_runtime_family = \"mlx-vlm\""));
    assert_eq!(
        toml::from_str::<FamilyDocument>(&body)
            .expect("parse family")
            .mlx_runtime_family,
        family
    );
}

#[test]
fn mlx_runtime_family_inference_follows_model_capability() {
    assert_eq!(
        infer_mlx_runtime_family(ModelFormat::Mlx, &[ModelCapability::Chat]),
        Some(MlxRuntimeFamily::Lm)
    );
    assert_eq!(
        infer_mlx_runtime_family(ModelFormat::Mlx, &[ModelCapability::VisionChat]),
        Some(MlxRuntimeFamily::Vlm)
    );
    assert_eq!(
        infer_mlx_runtime_family(ModelFormat::Mlx, &[ModelCapability::AudioTranscription]),
        Some(MlxRuntimeFamily::Audio)
    );
    assert_eq!(
        infer_mlx_runtime_family(ModelFormat::Mlx, &[ModelCapability::ImageGeneration]),
        Some(MlxRuntimeFamily::Diffusion)
    );
    assert_eq!(
        infer_mlx_runtime_family(ModelFormat::Safetensors, &[ModelCapability::Chat]),
        None
    );
    assert_eq!(
        infer_mlx_runtime_family(ModelFormat::Mlx, &[ModelCapability::Embedding]),
        None
    );
    assert_eq!(
        infer_mlx_runtime_family(
            ModelFormat::Mlx,
            &[ModelCapability::Chat, ModelCapability::VisionChat]
        ),
        None
    );
}

#[test]
fn model_capability_parser_accepts_known_values_and_rejects_unknown_values() {
    assert_eq!(
        " embedding ".parse::<ModelCapability>().expect("embedding"),
        ModelCapability::Embedding
    );
    assert_eq!(
        "RERANK".parse::<ModelCapability>().expect("rerank"),
        ModelCapability::Rerank
    );
    assert_eq!(
        " audio-transcription "
            .parse::<ModelCapability>()
            .expect("audio transcription"),
        ModelCapability::AudioTranscription
    );
    assert_eq!(
        "audio-speech"
            .parse::<ModelCapability>()
            .expect("audio speech"),
        ModelCapability::AudioSpeech
    );
    assert_eq!(
        "vision-chat"
            .parse::<ModelCapability>()
            .expect("vision chat"),
        ModelCapability::VisionChat
    );
    assert_eq!(
        "image-generation"
            .parse::<ModelCapability>()
            .expect("image generation"),
        ModelCapability::ImageGeneration
    );

    let err = "audio".parse::<ModelCapability>().expect_err("parse error");
    assert!(err.to_string().contains("unsupported model capability"));
    let err = "multimodal"
        .parse::<ModelCapability>()
        .expect_err("parse error");
    assert!(err.to_string().contains("unsupported model capability"));
    let err = "video-generation"
        .parse::<ModelCapability>()
        .expect_err("parse error");
    assert!(err.to_string().contains("unsupported model capability"));
}

#[test]
fn huggingface_repo_escape_matches_source_index_contract() {
    assert_eq!(
        escape_huggingface_repo_id("org/model-name"),
        "org--model-name"
    );
}

#[test]
fn model_ports_cover_layout_staging_snapshot_manifest_catalog_indexes_and_reference_guard() {
    let ports = FakeModelPorts;
    let layout = ModelStoreLayout::from_models_dir("/tmp/tentgent/models");
    let runtime_layout = PythonRuntimeLayout {
        project_dir: PathBuf::from("/tmp/tentgent/python"),
        env_dir: PathBuf::from("/tmp/tentgent/python-env"),
        source: PythonRuntimeSource::DevelopmentSource,
    };
    let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    let metadata = model_metadata_fixture(model_ref.clone());
    let manifest = ModelManifest {
        files: vec![manifest_entry("model.gguf", 42)],
    };
    let variant = ModelVariantMetadata {
        format: ModelFormat::Gguf,
        status: ModelVariantStatus::Imported,
        import_method: ModelImportMethod::Add,
        relative_source_path: SOURCE_DIRNAME.to_string(),
    };

    ports
        .ensure_model_store_layout(&layout)
        .expect("ensure layout");
    let staged = ports
        .create_staging_source(&layout, ModelImportMethod::Pull)
        .expect("staging");
    ports
        .copy_local_source(std::path::Path::new("/tmp/source"), &staged)
        .expect("copy source");

    let mut progress_events = Vec::new();
    let snapshot = ports
        .fetch_hf_snapshot(
            HfModelSnapshotRequest {
                runtime: runtime_layout,
                repo_id: "org/model".to_string(),
                revision: Some("main".to_string()),
                destination_dir: staged.source_dir.clone(),
                secret: None,
            },
            &mut |event| progress_events.push(event),
        )
        .expect("fetch snapshot");
    assert_eq!(snapshot.resolved_revision, "resolved-sha");
    assert_eq!(progress_events.len(), 1);

    let built_manifest = ports
        .build_manifest(&staged.source_dir)
        .expect("build manifest");
    assert_eq!(built_manifest.file_count(), 1);
    assert_eq!(
        ports
            .model_ref_for_manifest(&built_manifest)
            .expect("identity"),
        ModelRef::parse("f".repeat(64)).expect("fixture ref")
    );

    ports
        .save_model_metadata(&layout, &metadata)
        .expect("save model metadata");
    ports
        .save_model_manifest(&layout, &model_ref, &manifest)
        .expect("save manifest");
    ports
        .save_variant_metadata(&layout, &model_ref, &variant)
        .expect("save variant");
    assert_eq!(
        ports
            .load_model_metadata(&layout, &model_ref)
            .expect("load metadata")
            .model_ref,
        model_ref
    );
    assert_eq!(ports.list_models(&layout).expect("list").len(), 1);
    assert_eq!(
        ports
            .inspect_model(
                &layout,
                &ModelRefSelector::parse(model_ref.short_ref()).expect("selector"),
            )
            .expect("inspect")
            .metadata
            .model_ref,
        model_ref
    );

    let local_index_path = ports
        .save_local_source_index(
            &layout,
            &LocalModelSourceIndex {
                model_ref: model_ref.clone(),
                short_ref: model_ref.short_ref().to_string(),
                source_path: "/tmp/source".to_string(),
                imported_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("local index");
    assert_eq!(local_index_path, layout.local_index_path(&model_ref));

    let hf_index_path = ports
        .save_hf_source_index(
            &layout,
            &HfModelSourceIndex {
                model_ref: model_ref.clone(),
                short_ref: model_ref.short_ref().to_string(),
                source_repo: "org/model".to_string(),
                source_revision: "resolved-sha".to_string(),
                imported_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("hf index");
    assert_eq!(
        hf_index_path,
        layout.hf_index_path("org/model", "resolved-sha")
    );
    assert_eq!(
        ports
            .remove_source_indexes(&layout, &model_ref)
            .expect("remove indexes"),
        vec![layout.local_index_path(&model_ref)]
    );

    assert!(!ports
        .model_content_exists(&layout, &model_ref)
        .expect("content exists"));
    assert_eq!(
        ports
            .install_staged_source(&layout, &staged, &model_ref, ModelFormat::Gguf)
            .expect("install"),
        layout.variant_source_dir(&model_ref, ModelFormat::Gguf)
    );
    ports
        .remove_model_content(&layout, &model_ref)
        .expect("remove content");
    ports.discard_staging(&staged).expect("discard staging");

    assert_eq!(
        ports
            .server_refs_for_model(&runtime_layout_fixture(), &model_ref)
            .expect("server refs"),
        vec!["server-ref".to_string()]
    );
}

fn manifest_entry(relative_path: &str, size_bytes: u64) -> ModelManifestEntry {
    ModelManifestEntry {
        relative_path: relative_path.to_string(),
        size_bytes,
        sha256: "0".repeat(64),
    }
}

fn model_metadata_fixture(model_ref: ModelRef) -> ModelMetadata {
    ModelMetadata {
        short_ref: model_ref.short_ref().to_string(),
        model_ref,
        source_kind: ModelSourceKind::Local,
        source_repo: None,
        source_revision: None,
        source_path: Some("/tmp/source".to_string()),
        primary_format: ModelFormat::Gguf,
        detected_formats: vec![ModelFormat::Gguf],
        mlx_runtime_family: None,
        model_capabilities: default_model_capabilities(),
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 42,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    }
}

fn runtime_layout_fixture() -> RuntimeLayout {
    let home = PathBuf::from("/tmp/tentgent");
    RuntimeLayout {
        home_dir: home.clone(),
        data_root_dir: home.clone(),
        config_path: home.join("config.toml"),
        models_dir: home.join("models"),
        adapters_dir: home.join("adapters"),
        datasets_dir: home.join("datasets"),
        sessions_dir: home.join("sessions"),
        servers_dir: home.join("servers"),
        train_dir: home.join("train"),
        cache_dir: home.join("cache"),
        runtime_dir: home.join("runtime"),
        logs_dir: home.join("logs"),
        locks_dir: home.join("locks"),
        python_env_dir: home.join("runtime/python-env"),
        bootstrap_dir: home.join("runtime/bootstrap"),
        bootstrap_uv_dir: home.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: home.join("runtime/bootstrap/uv-cache"),
        capabilities_path: home.join("runtime/capabilities.toml"),
        auth_metadata_path: home.join("runtime/auth.toml"),
    }
}

struct FakeModelPorts;

impl ModelStoreLayoutInitializer for FakeModelPorts {
    fn ensure_model_store_layout(&self, _layout: &ModelStoreLayout) -> KernelResult<()> {
        Ok(())
    }
}

impl ModelSourceStager for FakeModelPorts {
    fn create_staging_source(
        &self,
        layout: &ModelStoreLayout,
        method: ModelImportMethod,
    ) -> KernelResult<StagedModelSource> {
        let staging_root = layout.staging_dir.join(method.as_str());
        Ok(StagedModelSource {
            source_dir: staging_root.join(SOURCE_DIRNAME),
            staging_root,
        })
    }

    fn copy_local_source(
        &self,
        _input_path: &std::path::Path,
        _staged: &StagedModelSource,
    ) -> KernelResult<()> {
        Ok(())
    }

    fn discard_staging(&self, _staged: &StagedModelSource) -> KernelResult<()> {
        Ok(())
    }
}

impl HfModelSnapshotFetcher for FakeModelPorts {
    fn fetch_hf_snapshot(
        &self,
        request: HfModelSnapshotRequest,
        progress: &mut dyn FnMut(HfModelPullProgress),
    ) -> KernelResult<HfModelSnapshot> {
        progress(HfModelPullProgress {
            description: request.repo_id.clone(),
            position: 1,
            total: Some(1),
            unit: "files".to_string(),
            finished: true,
        });

        Ok(HfModelSnapshot {
            repo_id: request.repo_id,
            resolved_revision: "resolved-sha".to_string(),
            local_dir: request.destination_dir,
            metadata: None,
        })
    }
}

impl ModelManifestBuilder for FakeModelPorts {
    fn build_manifest(&self, _source_root: &std::path::Path) -> KernelResult<ModelManifest> {
        Ok(ModelManifest {
            files: vec![manifest_entry("model.gguf", 42)],
        })
    }
}

impl ModelIdentityGenerator for FakeModelPorts {
    fn model_ref_for_manifest(&self, _manifest: &ModelManifest) -> KernelResult<ModelRef> {
        Ok(ModelRef::parse("f".repeat(64)).expect("fixture ref"))
    }
}

impl ModelCatalogStore for FakeModelPorts {
    fn list_models(
        &self,
        _layout: &ModelStoreLayout,
    ) -> KernelResult<Vec<super::domain::ModelSummary>> {
        let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
        Ok(vec![super::domain::ModelSummary {
            metadata: model_metadata_fixture(model_ref.clone()),
            store_path: PathBuf::from("/tmp/tentgent/models/store").join(model_ref.as_str()),
        }])
    }

    fn inspect_model(
        &self,
        layout: &ModelStoreLayout,
        _selector: &ModelRefSelector,
    ) -> KernelResult<super::domain::ModelInspection> {
        let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
        let metadata = model_metadata_fixture(model_ref.clone());
        Ok(super::domain::ModelInspection {
            store_path: layout.model_dir(&model_ref),
            manifest_path: layout.manifest_path(&model_ref),
            variant_source_path: layout.variant_source_dir(&model_ref, metadata.primary_format),
            metadata,
        })
    }

    fn load_model_metadata(
        &self,
        _layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<ModelMetadata> {
        Ok(model_metadata_fixture(model_ref.clone()))
    }

    fn save_model_metadata(
        &self,
        _layout: &ModelStoreLayout,
        _metadata: &ModelMetadata,
    ) -> KernelResult<()> {
        Ok(())
    }

    fn save_model_manifest(
        &self,
        _layout: &ModelStoreLayout,
        _model_ref: &ModelRef,
        _manifest: &ModelManifest,
    ) -> KernelResult<()> {
        Ok(())
    }

    fn save_variant_metadata(
        &self,
        _layout: &ModelStoreLayout,
        _model_ref: &ModelRef,
        _variant: &ModelVariantMetadata,
    ) -> KernelResult<()> {
        Ok(())
    }
}

impl ModelSourceIndexStore for FakeModelPorts {
    fn save_local_source_index(
        &self,
        layout: &ModelStoreLayout,
        index: &LocalModelSourceIndex,
    ) -> KernelResult<PathBuf> {
        Ok(layout.local_index_path(&index.model_ref))
    }

    fn save_hf_source_index(
        &self,
        layout: &ModelStoreLayout,
        index: &HfModelSourceIndex,
    ) -> KernelResult<PathBuf> {
        Ok(layout.hf_index_path(&index.source_repo, &index.source_revision))
    }

    fn remove_source_indexes(
        &self,
        layout: &ModelStoreLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<PathBuf>> {
        Ok(vec![layout.local_index_path(model_ref)])
    }
}

impl ModelContentStore for FakeModelPorts {
    fn model_content_exists(
        &self,
        _layout: &ModelStoreLayout,
        _model_ref: &ModelRef,
    ) -> KernelResult<bool> {
        Ok(false)
    }

    fn install_staged_source(
        &self,
        layout: &ModelStoreLayout,
        _staged: &StagedModelSource,
        model_ref: &ModelRef,
        format: ModelFormat,
    ) -> KernelResult<PathBuf> {
        Ok(layout.variant_source_dir(model_ref, format))
    }

    fn remove_model_content(
        &self,
        _layout: &ModelStoreLayout,
        _model_ref: &ModelRef,
    ) -> KernelResult<()> {
        Ok(())
    }
}

impl ModelServerReferenceProbe for FakeModelPorts {
    fn server_refs_for_model(
        &self,
        _layout: &RuntimeLayout,
        _model_ref: &ModelRef,
    ) -> KernelResult<Vec<String>> {
        Ok(vec!["server-ref".to_string()])
    }
}
