use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::adapter::domain::{
    AdapterBackendSupport, AdapterFormat, AdapterManifest, AdapterManifestEntry, AdapterMetadata,
    AdapterRef, AdapterRefSelector, AdapterSourceKind, AdapterStoreLayout, AdapterType,
    BaseModelAdapterIndex, HfAdapterSourceIndex, LocalAdapterSourceIndex,
    TrainRunAdapterSourceIndex, PEFT_ADAPTER_MODEL_FILENAME,
};
use crate::features::adapter::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterContentStore, AdapterIdentityGenerator,
    AdapterManifestBuilder, AdapterServerReferenceProbe, AdapterSourceIndexStore,
    AdapterSourceMetadataReader, AdapterSourceStager, AdapterStoreLayoutInitializer,
};
use crate::features::model::domain::ModelRef;
use crate::foundation::layout::RuntimeLayout;

use super::{
    FileAdapterBaseIndexStore, FileAdapterCatalogStore, FileAdapterContentStore,
    FileAdapterServerReferenceProbe, FileAdapterSourceIndexStore, StdAdapterIdentityGenerator,
    StdAdapterManifestBuilder, StdAdapterSourceMetadataReader, StdAdapterSourceStager,
    StdAdapterStoreLayoutInitializer,
};

#[test]
fn filesystem_adapter_infra_stages_metadata_manifests_installs_catalogs_and_removes_content() {
    let root = unique_path("adapter-infra-roundtrip");
    let layout = AdapterStoreLayout::from_adapters_dir(root.join("adapters"));
    let initializer = StdAdapterStoreLayoutInitializer;
    initializer
        .ensure_adapter_store_layout(&layout)
        .expect("ensure layout");

    let input = root.join("input");
    fs::create_dir_all(&input).expect("input dir");
    fs::write(input.join(PEFT_ADAPTER_MODEL_FILENAME), b"adapter bytes").expect("adapter weights");
    fs::write(
        input.join("adapter_config.json"),
        r#"{"base_model_name_or_path":"org/base","revision":"base-sha","model_type":"llama"}"#,
    )
    .expect("adapter config");

    let stager = StdAdapterSourceStager;
    let staged = stager
        .create_staging_source(&layout, AdapterSourceKind::Local)
        .expect("staging");
    stager
        .copy_local_source(&input, &staged)
        .expect("copy local source");

    let metadata_reader = StdAdapterSourceMetadataReader;
    let source_metadata = metadata_reader
        .read_source_metadata(&staged.source_dir)
        .expect("source metadata");
    assert_eq!(
        source_metadata.base_model_source_repo.as_deref(),
        Some("org/base")
    );
    assert_eq!(
        source_metadata.base_model_source_revision.as_deref(),
        Some("base-sha")
    );
    assert_eq!(source_metadata.model_family.as_deref(), Some("llama"));

    let manifest_builder = StdAdapterManifestBuilder;
    let manifest = manifest_builder
        .build_manifest(&staged.source_dir)
        .expect("manifest");
    assert_eq!(manifest.file_count(), 2);
    assert!(manifest.contains_path("adapter_config.json"));
    assert!(manifest.contains_path(PEFT_ADAPTER_MODEL_FILENAME));

    let identity = StdAdapterIdentityGenerator;
    let adapter_ref = identity
        .adapter_ref_for_manifest(&manifest)
        .expect("adapter ref");
    let metadata = metadata_fixture(adapter_ref.clone(), AdapterSourceKind::Local);

    let content = FileAdapterContentStore;
    assert!(!content
        .adapter_content_exists(&layout, &adapter_ref)
        .expect("content exists before"));
    let source_path = content
        .install_staged_source(&layout, &staged, &adapter_ref)
        .expect("install staged source");
    assert!(source_path.join(PEFT_ADAPTER_MODEL_FILENAME).is_file());
    assert!(content
        .adapter_content_exists(&layout, &adapter_ref)
        .expect("content exists after"));

    let catalog = FileAdapterCatalogStore;
    catalog
        .save_adapter_metadata(&layout, &metadata)
        .expect("save adapter metadata");
    catalog
        .save_adapter_manifest(&layout, &adapter_ref, &manifest)
        .expect("save manifest");

    assert_eq!(
        catalog
            .load_adapter_metadata(&layout, &adapter_ref)
            .expect("load metadata")
            .adapter_ref,
        adapter_ref
    );
    assert_eq!(catalog.list_adapters(&layout).expect("list").len(), 1);
    assert_eq!(
        catalog
            .inspect_adapter(
                &layout,
                &AdapterRefSelector::parse(adapter_ref.short_ref()).expect("selector"),
            )
            .expect("inspect")
            .source_path,
        layout.source_dir(&adapter_ref)
    );

    content
        .remove_adapter_content(&layout, &adapter_ref)
        .expect("remove content");
    assert!(!layout.adapter_dir(&adapter_ref).exists());
    stager.discard_staging(&staged).expect("discard staging");
}

#[test]
fn filesystem_adapter_indexes_write_and_remove_matching_adapter_refs() {
    let root = unique_path("adapter-index");
    let layout = AdapterStoreLayout::from_adapters_dir(root.join("adapters"));
    StdAdapterStoreLayoutInitializer
        .ensure_adapter_store_layout(&layout)
        .expect("ensure layout");
    let adapter_ref = AdapterRef::parse("a".repeat(64)).expect("adapter ref");
    let other_ref = AdapterRef::parse("b".repeat(64)).expect("other ref");
    let base_model_ref = ModelRef::parse("c".repeat(64)).expect("model ref");
    let other_base_model_ref = ModelRef::parse("d".repeat(64)).expect("other model ref");
    let source_store = FileAdapterSourceIndexStore;
    let base_store = FileAdapterBaseIndexStore;

    let local_path = source_store
        .save_local_source_index(
            &layout,
            &LocalAdapterSourceIndex {
                adapter_ref: adapter_ref.clone(),
                short_ref: adapter_ref.short_ref().to_string(),
                source_path: "/tmp/source".to_string(),
                imported_at: imported_at(),
            },
        )
        .expect("local index");
    let hf_path = source_store
        .save_hf_source_index(
            &layout,
            &HfAdapterSourceIndex {
                adapter_ref: adapter_ref.clone(),
                short_ref: adapter_ref.short_ref().to_string(),
                source_repo: "org/adapter".to_string(),
                source_revision: "resolved".to_string(),
                imported_at: imported_at(),
            },
        )
        .expect("hf index");
    let train_run_path = source_store
        .save_train_run_source_index(
            &layout,
            &TrainRunAdapterSourceIndex {
                adapter_ref: adapter_ref.clone(),
                short_ref: adapter_ref.short_ref().to_string(),
                training_run_ref: "run-ref".to_string(),
                training_dataset_ref: "dataset-ref".to_string(),
                training_config_ref: "config-ref".to_string(),
                imported_at: imported_at(),
            },
        )
        .expect("train-run index");
    source_store
        .save_hf_source_index(
            &layout,
            &HfAdapterSourceIndex {
                adapter_ref: other_ref.clone(),
                short_ref: other_ref.short_ref().to_string(),
                source_repo: "org/other".to_string(),
                source_revision: "resolved".to_string(),
                imported_at: imported_at(),
            },
        )
        .expect("other hf index");

    let removed = source_store
        .remove_source_indexes(&layout, &adapter_ref)
        .expect("remove source indexes");
    assert!(removed.contains(&local_path));
    assert!(removed.contains(&hf_path));
    assert!(removed.contains(&train_run_path));
    assert!(!local_path.exists());
    assert!(!hf_path.exists());
    assert!(!train_run_path.exists());
    assert!(layout.hf_index_path("org/other", "resolved").exists());

    let first_base_index = base_index(adapter_ref.clone(), base_model_ref.clone());
    let base_index_path = base_store
        .save_base_model_index(&layout, &first_base_index)
        .expect("base index");
    assert_eq!(
        base_store
            .remove_base_model_index(&layout, &first_base_index)
            .expect("remove one base index"),
        Some(base_index_path.clone())
    );
    assert!(!base_index_path.exists());

    let base_path = base_store
        .save_base_model_index(
            &layout,
            &base_index(adapter_ref.clone(), base_model_ref.clone()),
        )
        .expect("base index");
    let other_base_path = base_store
        .save_base_model_index(
            &layout,
            &base_index(adapter_ref.clone(), other_base_model_ref.clone()),
        )
        .expect("other base index");
    let untouched_base_path = base_store
        .save_base_model_index(
            &layout,
            &base_index(other_ref.clone(), base_model_ref.clone()),
        )
        .expect("untouched base index");

    let removed_base = base_store
        .remove_base_model_indexes(&layout, &adapter_ref)
        .expect("remove base indexes");
    assert!(removed_base.contains(&base_path));
    assert!(removed_base.contains(&other_base_path));
    assert!(!base_path.exists());
    assert!(!other_base_path.exists());
    assert!(untouched_base_path.exists());
}

#[test]
fn adapter_identity_generation_sorts_manifest_before_hashing() {
    let identity = StdAdapterIdentityGenerator;
    let unsorted = AdapterManifest {
        files: vec![
            manifest_entry("b.safetensors", 2),
            manifest_entry("a.json", 1),
        ],
    };
    let sorted = AdapterManifest {
        files: vec![
            manifest_entry("a.json", 1),
            manifest_entry("b.safetensors", 2),
        ],
    };

    assert_eq!(
        identity
            .adapter_ref_for_manifest(&unsorted)
            .expect("unsorted"),
        identity.adapter_ref_for_manifest(&sorted).expect("sorted")
    );
}

#[test]
fn server_reference_probe_reports_adapter_removal_blockers() {
    let root = unique_path("adapter-server-ref");
    let layout = runtime_layout(root.as_path());
    let adapter_ref = AdapterRef::parse("e".repeat(64)).expect("adapter ref");

    let server_a = layout.servers_dir.join("server-a");
    fs::create_dir_all(&server_a).expect("server a dir");
    fs::write(
        server_a.join("server.toml"),
        format!(
            r#"
short_ref = "server-a"
adapter_ref = "{}"
"#,
            adapter_ref.short_ref()
        ),
    )
    .expect("server a spec");

    let server_b = layout.servers_dir.join("server-b");
    fs::create_dir_all(&server_b).expect("server b dir");
    fs::write(
        server_b.join("server.toml"),
        format!(
            r#"
allowed_adapters = ["{}"]
"#,
            adapter_ref.as_str()
        ),
    )
    .expect("server b spec");

    let server_c = layout.servers_dir.join("server-c");
    fs::create_dir_all(&server_c).expect("server c dir");
    fs::write(
        server_c.join("server.toml"),
        r#"
short_ref = "server-c"
adapter_ref = "ffffffffffff"
"#,
    )
    .expect("server c spec");

    let refs = FileAdapterServerReferenceProbe
        .server_refs_for_adapter(&layout, &adapter_ref)
        .expect("server refs");
    assert_eq!(refs, vec!["server-a".to_string(), "server-b".to_string()]);
}

fn metadata_fixture(adapter_ref: AdapterRef, source_kind: AdapterSourceKind) -> AdapterMetadata {
    AdapterMetadata {
        short_ref: adapter_ref.short_ref().to_string(),
        adapter_ref,
        adapter_format: AdapterFormat::Peft,
        adapter_type: AdapterType::Lora,
        base_model_ref: Some(ModelRef::parse("c".repeat(64)).expect("model ref")),
        base_model_source_repo: Some("org/base".to_string()),
        base_model_source_revision: Some("base-sha".to_string()),
        model_family: Some("llama".to_string()),
        backend_support: vec![AdapterBackendSupport::TransformersPeft],
        source_kind,
        source_repo: None,
        source_revision: None,
        source_path: Some("/tmp/source".to_string()),
        training_dataset_ref: None,
        training_run_ref: None,
        training_config_ref: None,
        file_count: 2,
        total_bytes: 11,
        imported_at: imported_at(),
    }
}

fn base_index(adapter_ref: AdapterRef, base_model_ref: ModelRef) -> BaseModelAdapterIndex {
    BaseModelAdapterIndex {
        short_ref: adapter_ref.short_ref().to_string(),
        adapter_ref,
        base_model_ref,
        adapter_format: AdapterFormat::Peft,
        imported_at: imported_at(),
    }
}

fn manifest_entry(relative_path: &str, size_bytes: u64) -> AdapterManifestEntry {
    AdapterManifestEntry {
        relative_path: relative_path.to_string(),
        size_bytes,
        sha256: "0".repeat(64),
    }
}

fn imported_at() -> String {
    "2026-05-17T00:00:00Z".to_string()
}

fn unique_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{label}-{nanos}"))
}

fn runtime_layout(home: &Path) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: home.to_path_buf(),
        data_root_dir: home.to_path_buf(),
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
