use std::path::{Path, PathBuf};

use crate::features::model::domain::{ModelCapability, ModelRef};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeSource};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::domain::{
    backend_support_for_format, detect_adapter_format, escape_huggingface_repo_id,
    validate_adapter_compatibility, AdapterBackendSupport, AdapterCompatibilityError,
    AdapterCompatibilityTarget, AdapterFormat, AdapterInspection, AdapterManifest,
    AdapterManifestEntry, AdapterMetadata, AdapterRef, AdapterRefParseError, AdapterRefSelector,
    AdapterSourceKind, AdapterStoreLayout, AdapterSummary, AdapterType, BaseModelAdapterIndex,
    HfAdapterPullProgress, HfAdapterSourceIndex, LocalAdapterSourceIndex,
    TrainRunAdapterSourceIndex, ADAPTER_MANIFEST_FILENAME, ADAPTER_METADATA_FILENAME,
    HUGGINGFACE_SOURCE_DIRNAME, LOCAL_SOURCE_DIRNAME, MLX_ADAPTERS_FILENAME,
    PEFT_ADAPTER_MODEL_FILENAME, SHORT_ADAPTER_REF_LENGTH, SOURCE_DIRNAME, STAGING_DIRNAME,
    STORE_DIRNAME, TRAIN_RUN_SOURCE_DIRNAME,
};
use super::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterContentStore, AdapterIdentityGenerator,
    AdapterManifestBuilder, AdapterServerReferenceProbe, AdapterSourceIndexStore,
    AdapterSourceMetadata, AdapterSourceMetadataReader, AdapterSourceStager,
    AdapterStoreLayoutInitializer, HfAdapterSnapshot, HfAdapterSnapshotFetcher,
    HfAdapterSnapshotRequest, StagedAdapterSource,
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

#[test]
fn adapter_ports_cover_layout_staging_snapshot_manifest_catalog_indexes_and_reference_guard() {
    let ports = FakeAdapterPorts;
    let layout = AdapterStoreLayout::from_adapters_dir("/tmp/tentgent/adapters");
    let runtime_layout = PythonRuntimeLayout {
        project_dir: PathBuf::from("/tmp/tentgent/python"),
        env_dir: PathBuf::from("/tmp/tentgent/python-env"),
        source: PythonRuntimeSource::DevelopmentSource,
    };
    let adapter_ref = AdapterRef::parse("d".repeat(64)).expect("adapter ref");
    let base_model_ref = ModelRef::parse("b".repeat(64)).expect("model ref");
    let metadata = metadata_fixture(Some(base_model_ref.clone()));
    let manifest = AdapterManifest {
        files: vec![manifest_entry(PEFT_ADAPTER_MODEL_FILENAME, 42)],
    };

    ports
        .ensure_adapter_store_layout(&layout)
        .expect("ensure layout");
    let staged = ports
        .create_staging_source(&layout, AdapterSourceKind::HuggingFace)
        .expect("staging");
    ports
        .copy_local_source(Path::new("/tmp/source"), &staged)
        .expect("copy source");

    let mut progress_events = Vec::new();
    let snapshot = ports
        .fetch_hf_snapshot(
            HfAdapterSnapshotRequest {
                runtime: runtime_layout,
                repo_id: "org/adapter".to_string(),
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
            .adapter_ref_for_manifest(&built_manifest)
            .expect("identity"),
        AdapterRef::parse("f".repeat(64)).expect("fixture ref")
    );

    let source_metadata = ports
        .read_source_metadata(&staged.source_dir)
        .expect("source metadata");
    assert_eq!(
        source_metadata.base_model_source_repo.as_deref(),
        Some("org/base")
    );

    ports
        .save_adapter_metadata(&layout, &metadata)
        .expect("save adapter metadata");
    ports
        .save_adapter_manifest(&layout, &adapter_ref, &manifest)
        .expect("save manifest");
    assert_eq!(
        ports
            .load_adapter_metadata(&layout, &adapter_ref)
            .expect("load metadata")
            .adapter_ref,
        adapter_ref
    );
    assert_eq!(ports.list_adapters(&layout).expect("list").len(), 1);
    assert_eq!(
        ports
            .inspect_adapter(
                &layout,
                &AdapterRefSelector::parse(adapter_ref.short_ref()).expect("selector"),
            )
            .expect("inspect")
            .metadata
            .adapter_ref,
        adapter_ref
    );

    let local_index_path = ports
        .save_local_source_index(
            &layout,
            &LocalAdapterSourceIndex {
                adapter_ref: adapter_ref.clone(),
                short_ref: adapter_ref.short_ref().to_string(),
                source_path: "/tmp/source".to_string(),
                imported_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("local index");
    assert_eq!(local_index_path, layout.local_index_path(&adapter_ref));

    let hf_index_path = ports
        .save_hf_source_index(
            &layout,
            &HfAdapterSourceIndex {
                adapter_ref: adapter_ref.clone(),
                short_ref: adapter_ref.short_ref().to_string(),
                source_repo: "org/adapter".to_string(),
                source_revision: "resolved-sha".to_string(),
                imported_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("hf index");
    assert_eq!(
        hf_index_path,
        layout.hf_index_path("org/adapter", "resolved-sha")
    );

    let train_run_index_path = ports
        .save_train_run_source_index(
            &layout,
            &TrainRunAdapterSourceIndex {
                adapter_ref: adapter_ref.clone(),
                short_ref: adapter_ref.short_ref().to_string(),
                training_run_ref: "run-ref".to_string(),
                training_dataset_ref: "dataset-ref".to_string(),
                training_config_ref: "config-ref".to_string(),
                imported_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .expect("train-run index");
    assert_eq!(train_run_index_path, layout.train_run_index_path("run-ref"));
    assert_eq!(
        ports
            .remove_source_indexes(&layout, &adapter_ref)
            .expect("remove indexes"),
        vec![layout.local_index_path(&adapter_ref)]
    );

    let base_index = BaseModelAdapterIndex {
        adapter_ref: adapter_ref.clone(),
        short_ref: adapter_ref.short_ref().to_string(),
        base_model_ref: base_model_ref.clone(),
        adapter_format: AdapterFormat::Peft,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
    };
    let base_index_path = ports
        .save_base_model_index(&layout, &base_index)
        .expect("base index");
    assert_eq!(
        base_index_path,
        layout.base_index_path(&base_model_ref, &adapter_ref)
    );
    assert_eq!(
        ports
            .remove_base_model_index(&layout, &base_index)
            .expect("remove one base index"),
        Some(layout.base_index_path(&base_model_ref, &adapter_ref))
    );
    assert_eq!(
        ports
            .remove_base_model_indexes(&layout, &adapter_ref)
            .expect("remove base indexes"),
        vec![layout.base_index_path(&base_model_ref, &adapter_ref)]
    );

    assert!(!ports
        .adapter_content_exists(&layout, &adapter_ref)
        .expect("content exists"));
    assert_eq!(
        ports
            .install_staged_source(&layout, &staged, &adapter_ref)
            .expect("install"),
        layout.source_dir(&adapter_ref)
    );
    ports
        .remove_adapter_content(&layout, &adapter_ref)
        .expect("remove content");
    ports.discard_staging(&staged).expect("discard staging");

    assert_eq!(
        ports
            .server_refs_for_adapter(&runtime_layout_fixture(), &adapter_ref)
            .expect("server refs"),
        vec!["server-ref".to_string()]
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

struct FakeAdapterPorts;

impl AdapterStoreLayoutInitializer for FakeAdapterPorts {
    fn ensure_adapter_store_layout(&self, _layout: &AdapterStoreLayout) -> KernelResult<()> {
        Ok(())
    }
}

impl AdapterSourceStager for FakeAdapterPorts {
    fn create_staging_source(
        &self,
        layout: &AdapterStoreLayout,
        source_kind: AdapterSourceKind,
    ) -> KernelResult<StagedAdapterSource> {
        let staging_root = layout.staging_dir.join(source_kind.as_str());
        Ok(StagedAdapterSource {
            source_dir: staging_root.join(SOURCE_DIRNAME),
            staging_root,
        })
    }

    fn copy_local_source(
        &self,
        _input_path: &Path,
        _staged: &StagedAdapterSource,
    ) -> KernelResult<()> {
        Ok(())
    }

    fn discard_staging(&self, _staged: &StagedAdapterSource) -> KernelResult<()> {
        Ok(())
    }
}

impl HfAdapterSnapshotFetcher for FakeAdapterPorts {
    fn fetch_hf_snapshot(
        &self,
        request: HfAdapterSnapshotRequest,
        progress: &mut dyn FnMut(HfAdapterPullProgress),
    ) -> KernelResult<HfAdapterSnapshot> {
        progress(HfAdapterPullProgress {
            description: request.repo_id.clone(),
            position: 1,
            total: Some(1),
            unit: "files".to_string(),
            finished: true,
        });

        Ok(HfAdapterSnapshot {
            repo_id: request.repo_id,
            resolved_revision: "resolved-sha".to_string(),
            local_dir: request.destination_dir,
        })
    }
}

impl AdapterManifestBuilder for FakeAdapterPorts {
    fn build_manifest(&self, _source_root: &Path) -> KernelResult<AdapterManifest> {
        Ok(AdapterManifest {
            files: vec![manifest_entry(PEFT_ADAPTER_MODEL_FILENAME, 42)],
        })
    }
}

impl AdapterIdentityGenerator for FakeAdapterPorts {
    fn adapter_ref_for_manifest(&self, _manifest: &AdapterManifest) -> KernelResult<AdapterRef> {
        Ok(AdapterRef::parse("f".repeat(64)).expect("fixture ref"))
    }
}

impl AdapterSourceMetadataReader for FakeAdapterPorts {
    fn read_source_metadata(&self, _source_root: &Path) -> KernelResult<AdapterSourceMetadata> {
        Ok(AdapterSourceMetadata {
            base_model_source_repo: Some("org/base".to_string()),
            base_model_source_revision: Some("base-sha".to_string()),
            model_family: Some("llama".to_string()),
        })
    }
}

impl AdapterCatalogStore for FakeAdapterPorts {
    fn list_adapters(&self, _layout: &AdapterStoreLayout) -> KernelResult<Vec<AdapterSummary>> {
        let adapter_ref = AdapterRef::parse("d".repeat(64)).expect("adapter ref");
        Ok(vec![AdapterSummary {
            metadata: metadata_fixture(Some(ModelRef::parse("b".repeat(64)).expect("model ref"))),
            store_path: PathBuf::from("/tmp/tentgent/adapters/store").join(adapter_ref.as_str()),
        }])
    }

    fn inspect_adapter(
        &self,
        layout: &AdapterStoreLayout,
        _selector: &AdapterRefSelector,
    ) -> KernelResult<AdapterInspection> {
        let adapter_ref = AdapterRef::parse("d".repeat(64)).expect("adapter ref");
        let mut metadata =
            metadata_fixture(Some(ModelRef::parse("b".repeat(64)).expect("model ref")));
        metadata.short_ref = adapter_ref.short_ref().to_string();
        metadata.adapter_ref = adapter_ref.clone();
        Ok(AdapterInspection {
            store_path: layout.adapter_dir(&adapter_ref),
            manifest_path: layout.manifest_path(&adapter_ref),
            source_path: layout.source_dir(&adapter_ref),
            metadata,
        })
    }

    fn load_adapter_metadata(
        &self,
        _layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<AdapterMetadata> {
        let mut metadata =
            metadata_fixture(Some(ModelRef::parse("b".repeat(64)).expect("model ref")));
        metadata.short_ref = adapter_ref.short_ref().to_string();
        metadata.adapter_ref = adapter_ref.clone();
        Ok(metadata)
    }

    fn save_adapter_metadata(
        &self,
        _layout: &AdapterStoreLayout,
        _metadata: &AdapterMetadata,
    ) -> KernelResult<()> {
        Ok(())
    }

    fn save_adapter_manifest(
        &self,
        _layout: &AdapterStoreLayout,
        _adapter_ref: &AdapterRef,
        _manifest: &AdapterManifest,
    ) -> KernelResult<()> {
        Ok(())
    }
}

impl AdapterSourceIndexStore for FakeAdapterPorts {
    fn save_local_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &LocalAdapterSourceIndex,
    ) -> KernelResult<PathBuf> {
        Ok(layout.local_index_path(&index.adapter_ref))
    }

    fn save_hf_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &HfAdapterSourceIndex,
    ) -> KernelResult<PathBuf> {
        Ok(layout.hf_index_path(&index.source_repo, &index.source_revision))
    }

    fn save_train_run_source_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &TrainRunAdapterSourceIndex,
    ) -> KernelResult<PathBuf> {
        Ok(layout.train_run_index_path(&index.training_run_ref))
    }

    fn remove_source_indexes(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<PathBuf>> {
        Ok(vec![layout.local_index_path(adapter_ref)])
    }
}

impl AdapterBaseIndexStore for FakeAdapterPorts {
    fn save_base_model_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &BaseModelAdapterIndex,
    ) -> KernelResult<PathBuf> {
        Ok(layout.base_index_path(&index.base_model_ref, &index.adapter_ref))
    }

    fn remove_base_model_index(
        &self,
        layout: &AdapterStoreLayout,
        index: &BaseModelAdapterIndex,
    ) -> KernelResult<Option<PathBuf>> {
        Ok(Some(layout.base_index_path(
            &index.base_model_ref,
            &index.adapter_ref,
        )))
    }

    fn remove_base_model_indexes(
        &self,
        layout: &AdapterStoreLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<PathBuf>> {
        let base_model_ref = ModelRef::parse("b".repeat(64)).expect("model ref");
        Ok(vec![layout.base_index_path(&base_model_ref, adapter_ref)])
    }
}

impl AdapterContentStore for FakeAdapterPorts {
    fn adapter_content_exists(
        &self,
        _layout: &AdapterStoreLayout,
        _adapter_ref: &AdapterRef,
    ) -> KernelResult<bool> {
        Ok(false)
    }

    fn install_staged_source(
        &self,
        layout: &AdapterStoreLayout,
        _staged: &StagedAdapterSource,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<PathBuf> {
        Ok(layout.source_dir(adapter_ref))
    }

    fn remove_adapter_content(
        &self,
        _layout: &AdapterStoreLayout,
        _adapter_ref: &AdapterRef,
    ) -> KernelResult<()> {
        Ok(())
    }
}

impl AdapterServerReferenceProbe for FakeAdapterPorts {
    fn server_refs_for_adapter(
        &self,
        _layout: &RuntimeLayout,
        _adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<String>> {
        Ok(vec!["server-ref".to_string()])
    }
}
