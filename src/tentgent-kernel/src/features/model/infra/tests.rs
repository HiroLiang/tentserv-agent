use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::model::domain::{
    HfModelSourceIndex, LocalModelSourceIndex, ModelFormat, ModelImportMethod, ModelManifest,
    ModelManifestEntry, ModelMetadata, ModelRef, ModelRefSelector, ModelSourceKind,
    ModelStoreLayout, ModelVariantMetadata, ModelVariantStatus, SOURCE_DIRNAME,
};
use crate::features::model::ports::{
    ModelCatalogStore, ModelContentStore, ModelIdentityGenerator, ModelManifestBuilder,
    ModelServerReferenceProbe, ModelSourceIndexStore, ModelSourceStager,
    ModelStoreLayoutInitializer,
};
use crate::foundation::layout::RuntimeLayout;

use super::{
    FileModelCatalogStore, FileModelContentStore, FileModelServerReferenceProbe,
    FileModelSourceIndexStore, StdModelIdentityGenerator, StdModelManifestBuilder,
    StdModelSourceStager, StdModelStoreLayoutInitializer,
};

#[test]
fn filesystem_model_infra_stages_manifests_installs_catalogs_and_removes_model_content() {
    let root = unique_path("model-infra-roundtrip");
    let layout = ModelStoreLayout::from_models_dir(root.join("models"));
    let initializer = StdModelStoreLayoutInitializer;
    initializer
        .ensure_model_store_layout(&layout)
        .expect("ensure layout");

    let input = root.join("input");
    fs::create_dir_all(input.join("nested")).expect("input dir");
    fs::write(input.join("nested/model.gguf"), b"model bytes").expect("model file");

    let stager = StdModelSourceStager;
    let staged = stager
        .create_staging_source(&layout, ModelImportMethod::Add)
        .expect("staging");
    stager
        .copy_local_source(&input, &staged)
        .expect("copy local source");

    let manifest_builder = StdModelManifestBuilder;
    let manifest = manifest_builder
        .build_manifest(&staged.source_dir)
        .expect("manifest");
    assert_eq!(manifest.file_count(), 1);
    assert_eq!(manifest.files[0].relative_path, "nested/model.gguf");

    let identity = StdModelIdentityGenerator;
    let model_ref = identity
        .model_ref_for_manifest(&manifest)
        .expect("model ref");
    let metadata = metadata_fixture(model_ref.clone());
    let variant = variant_fixture();

    let content = FileModelContentStore;
    assert!(!content
        .model_content_exists(&layout, &model_ref)
        .expect("content exists before"));
    let source_path = content
        .install_staged_source(&layout, &staged, &model_ref, ModelFormat::Gguf)
        .expect("install staged source");
    assert!(source_path.join("nested/model.gguf").is_file());
    assert!(content
        .model_content_exists(&layout, &model_ref)
        .expect("content exists after"));

    let catalog = FileModelCatalogStore;
    catalog
        .save_model_metadata(&layout, &metadata)
        .expect("save model metadata");
    catalog
        .save_model_manifest(&layout, &model_ref, &manifest)
        .expect("save manifest");
    catalog
        .save_variant_metadata(&layout, &model_ref, &variant)
        .expect("save variant");

    assert_eq!(
        catalog
            .load_model_metadata(&layout, &model_ref)
            .expect("load metadata")
            .model_ref,
        model_ref
    );
    assert_eq!(catalog.list_models(&layout).expect("list").len(), 1);
    assert_eq!(
        catalog
            .inspect_model(
                &layout,
                &ModelRefSelector::parse(model_ref.short_ref()).expect("selector"),
            )
            .expect("inspect")
            .variant_source_path,
        layout.variant_source_dir(&model_ref, ModelFormat::Gguf)
    );

    content
        .remove_model_content(&layout, &model_ref)
        .expect("remove content");
    assert!(!layout.model_dir(&model_ref).exists());
    stager.discard_staging(&staged).expect("discard staging");
}

#[test]
fn filesystem_model_source_indexes_write_and_remove_matching_model_refs() {
    let root = unique_path("model-index");
    let layout = ModelStoreLayout::from_models_dir(root.join("models"));
    StdModelStoreLayoutInitializer
        .ensure_model_store_layout(&layout)
        .expect("ensure layout");
    let model_ref = ModelRef::parse("a".repeat(64)).expect("model ref");
    let other_ref = ModelRef::parse("b".repeat(64)).expect("other ref");
    let store = FileModelSourceIndexStore;

    let local_path = store
        .save_local_source_index(
            &layout,
            &LocalModelSourceIndex {
                model_ref: model_ref.clone(),
                short_ref: model_ref.short_ref().to_string(),
                source_path: "/tmp/source".to_string(),
                imported_at: imported_at(),
            },
        )
        .expect("local index");
    let hf_path = store
        .save_hf_source_index(
            &layout,
            &HfModelSourceIndex {
                model_ref: model_ref.clone(),
                short_ref: model_ref.short_ref().to_string(),
                source_repo: "org/model".to_string(),
                source_revision: "resolved".to_string(),
                imported_at: imported_at(),
            },
        )
        .expect("hf index");
    store
        .save_hf_source_index(
            &layout,
            &HfModelSourceIndex {
                model_ref: other_ref.clone(),
                short_ref: other_ref.short_ref().to_string(),
                source_repo: "org/other".to_string(),
                source_revision: "resolved".to_string(),
                imported_at: imported_at(),
            },
        )
        .expect("other hf index");

    let removed = store
        .remove_source_indexes(&layout, &model_ref)
        .expect("remove indexes");
    assert!(removed.contains(&local_path));
    assert!(removed.contains(&hf_path));
    assert!(!local_path.exists());
    assert!(!hf_path.exists());
    assert!(layout.hf_index_path("org/other", "resolved").exists());
}

#[test]
fn identity_generation_sorts_manifest_before_hashing() {
    let identity = StdModelIdentityGenerator;
    let unsorted = ModelManifest {
        files: vec![manifest_entry("b.gguf", 2), manifest_entry("a.gguf", 1)],
    };
    let sorted = ModelManifest {
        files: vec![manifest_entry("a.gguf", 1), manifest_entry("b.gguf", 2)],
    };

    assert_eq!(
        identity
            .model_ref_for_manifest(&unsorted)
            .expect("unsorted"),
        identity.model_ref_for_manifest(&sorted).expect("sorted")
    );
}

#[test]
fn server_reference_probe_reports_model_removal_blockers() {
    let root = unique_path("model-server-ref");
    let layout = runtime_layout(root.as_path());
    let model_ref = ModelRef::parse("c".repeat(64)).expect("model ref");
    let server_dir = layout.servers_dir.join("server-a");
    fs::create_dir_all(&server_dir).expect("server dir");
    fs::write(
        server_dir.join("server.toml"),
        format!(
            r#"
short_ref = "server-a"
model_ref = "{}"
"#,
            model_ref.short_ref()
        ),
    )
    .expect("server spec");

    let refs = FileModelServerReferenceProbe
        .server_refs_for_model(&layout, &model_ref)
        .expect("server refs");
    assert_eq!(refs, vec!["server-a".to_string()]);
}

fn metadata_fixture(model_ref: ModelRef) -> ModelMetadata {
    ModelMetadata {
        short_ref: model_ref.short_ref().to_string(),
        model_ref,
        source_kind: ModelSourceKind::Local,
        source_repo: None,
        source_revision: None,
        source_path: Some("/tmp/source".to_string()),
        primary_format: ModelFormat::Gguf,
        detected_formats: vec![ModelFormat::Gguf],
        file_count: 1,
        total_bytes: 11,
        imported_at: imported_at(),
    }
}

fn variant_fixture() -> ModelVariantMetadata {
    ModelVariantMetadata {
        format: ModelFormat::Gguf,
        status: ModelVariantStatus::Imported,
        import_method: ModelImportMethod::Add,
        relative_source_path: SOURCE_DIRNAME.to_string(),
    }
}

fn manifest_entry(relative_path: &str, size_bytes: u64) -> ModelManifestEntry {
    ModelManifestEntry {
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
