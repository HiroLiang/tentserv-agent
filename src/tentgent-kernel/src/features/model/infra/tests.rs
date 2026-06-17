use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::model::domain::{
    default_model_capabilities, default_model_capability_source, HfModelSourceIndex,
    LocalModelSourceIndex, ModelCapability, ModelCapabilityProof, ModelCapabilityProofSource,
    ModelCapabilityProofStatus, ModelFormat, ModelImportMethod, ModelManifest, ModelManifestEntry,
    ModelMetadata, ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
    ModelVariantMetadata, ModelVariantStatus, SOURCE_DIRNAME,
};
use crate::features::model::ports::{
    ModelCapabilityProofStore, ModelCatalogStore, ModelContentStore, ModelIdentityGenerator,
    ModelManifestBuilder, ModelServerReferenceProbe, ModelSourceIndexStore, ModelSourceStager,
    ModelStoreLayoutInitializer,
};
use crate::foundation::layout::RuntimeLayout;

use super::{
    FileModelCapabilityProofStore, FileModelCatalogStore, FileModelContentStore,
    FileModelServerReferenceProbe, FileModelSourceIndexStore, StdModelIdentityGenerator,
    StdModelManifestBuilder, StdModelSourceStager, StdModelStoreLayoutInitializer,
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

#[test]
fn filesystem_model_capability_proofs_keep_tuple_specific_records() {
    let root = unique_path("model-support-proof");
    let layout = ModelStoreLayout::from_models_dir(root.join("models"));
    let model_ref = ModelRef::parse("d".repeat(64)).expect("model ref");
    let store = FileModelCapabilityProofStore;

    let gguf = proof_fixture(
        model_ref.clone(),
        ModelCapability::Chat,
        "gguf",
        ModelCapabilityProofStatus::Verified,
        None,
    );
    let llama = proof_fixture(
        model_ref.clone(),
        ModelCapability::Chat,
        "llama-cpp",
        ModelCapabilityProofStatus::Failed,
        Some("runtime failed".to_string()),
    );

    store
        .save_capability_proof(&layout, &gguf)
        .expect("save gguf proof");
    store
        .save_capability_proof(&layout, &llama)
        .expect("save llama proof");

    let proofs = store
        .list_capability_proofs(&layout, &model_ref)
        .expect("list proofs");
    assert_eq!(proofs.len(), 2);
    assert!(proofs.iter().any(
        |proof| proof.backend == "gguf" && proof.status == ModelCapabilityProofStatus::Verified
    ));
    assert!(proofs.iter().any(|proof| proof.backend == "llama-cpp"
        && proof.status == ModelCapabilityProofStatus::Failed
        && proof.error.as_deref() == Some("runtime failed")));
    assert!(layout.support_proofs_dir(&model_ref).is_dir());
    assert!(layout
        .capability_proof_path(&model_ref, ModelCapability::Chat)
        .is_file());

    let overwritten = proof_fixture(
        model_ref.clone(),
        ModelCapability::Chat,
        "gguf",
        ModelCapabilityProofStatus::Failed,
        Some("new failure".to_string()),
    );
    store
        .save_capability_proof(&layout, &overwritten)
        .expect("overwrite gguf proof");

    let proofs = store
        .list_capability_proofs(&layout, &model_ref)
        .expect("list overwritten proofs");
    assert_eq!(proofs.len(), 2);
    assert!(proofs.iter().any(|proof| proof.backend == "gguf"
        && proof.status == ModelCapabilityProofStatus::Failed
        && proof.error.as_deref() == Some("new failure")));

    let mut profiled = proof_fixture(
        model_ref.clone(),
        ModelCapability::Chat,
        "gguf",
        ModelCapabilityProofStatus::Verified,
        None,
    );
    profiled.runtime_profile = Some("local-chat-llama-cpp".to_string());
    profiled.runtime_profile_version = Some(1);
    store
        .save_capability_proof(&layout, &profiled)
        .expect("save profiled proof");

    let proofs = store
        .list_capability_proofs(&layout, &model_ref)
        .expect("list profiled proofs");
    assert_eq!(proofs.len(), 3);
    assert!(proofs.iter().any(|proof| proof.backend == "gguf"
        && proof.runtime_profile.as_deref() == Some("local-chat-llama-cpp")
        && proof.runtime_profile_version == Some(1)
        && proof.status == ModelCapabilityProofStatus::Verified));

    let mut profiled_v2 = profiled.clone();
    profiled_v2.runtime_profile_version = Some(2);
    store
        .save_capability_proof(&layout, &profiled_v2)
        .expect("save profile version proof");

    let proofs = store
        .list_capability_proofs(&layout, &model_ref)
        .expect("list profile version proofs");
    assert_eq!(proofs.len(), 4);

    let mut profiled_overwrite = profiled_v2;
    profiled_overwrite.status = ModelCapabilityProofStatus::Failed;
    profiled_overwrite.error = Some("profile v2 failed".to_string());
    store
        .save_capability_proof(&layout, &profiled_overwrite)
        .expect("overwrite profile version proof");

    let proofs = store
        .list_capability_proofs(&layout, &model_ref)
        .expect("list overwritten profile version proofs");
    assert_eq!(proofs.len(), 4);
    assert!(proofs.iter().any(|proof| proof.backend == "gguf"
        && proof.runtime_profile.as_deref() == Some("local-chat-llama-cpp")
        && proof.runtime_profile_version == Some(2)
        && proof.status == ModelCapabilityProofStatus::Failed
        && proof.error.as_deref() == Some("profile v2 failed")));

    store
        .remove_capability_proof(&layout, &model_ref, ModelCapability::Chat)
        .expect("remove chat proofs");
    let proofs = store
        .list_capability_proofs(&layout, &model_ref)
        .expect("list after proof removal");
    assert!(proofs.is_empty());
    assert!(!layout
        .capability_proof_path(&model_ref, ModelCapability::Chat)
        .exists());
    assert!(!layout
        .support_proofs_capability_dir(&model_ref, ModelCapability::Chat)
        .exists());

    let _ = fs::remove_dir_all(root);
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
        mlx_runtime_family: None,
        model_capabilities: default_model_capabilities(),
        model_capability_source: default_model_capability_source(),
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

fn proof_fixture(
    model_ref: ModelRef,
    capability: ModelCapability,
    backend: impl Into<String>,
    status: ModelCapabilityProofStatus,
    error: Option<String>,
) -> ModelCapabilityProof {
    ModelCapabilityProof {
        model_ref,
        capability,
        status,
        source: ModelCapabilityProofSource::ServerStart,
        primary_format: ModelFormat::Gguf,
        mlx_runtime_family: None,
        backend: backend.into(),
        runtime_version: None,
        runtime_profile: None,
        runtime_profile_version: None,
        server_ref: Some("server-ref".to_string()),
        checked_at: imported_at(),
        error,
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
