use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::dataset::domain::{
    DatasetFormat, DatasetMetadata, DatasetPromptSource, DatasetRef, DatasetRefSelector,
    DatasetSourceKind, DatasetSplitKind, DatasetStoreLayout, DatasetSynthCounts,
    DatasetSynthPromptRequest, DatasetTemplateRequest, LocalDatasetSourceIndex,
};
use crate::features::dataset::ports::{
    DatasetCatalogStore, DatasetContentStore, DatasetDiffTarget, DatasetDiffer,
    DatasetIdentityGenerator, DatasetManifestBuilder, DatasetPackageDetector,
    DatasetReferenceGuard, DatasetSourceIndexStore, DatasetSourceStager,
    DatasetStoreLayoutInitializer, DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeClient,
    DatasetTemplateRenderer, DatasetValidator,
};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeSource};
use crate::foundation::layout::RuntimeLayout;

use super::{
    CloudDatasetSynthRuntimeClient, FileDatasetCatalogStore, FileDatasetContentStore,
    FileDatasetReferenceGuard, FileDatasetSourceIndexStore, MarkdownDatasetTemplateRenderer,
    StdDatasetDiffer, StdDatasetIdentityGenerator, StdDatasetManifestBuilder,
    StdDatasetPackageDetector, StdDatasetSourceStager, StdDatasetStoreLayoutInitializer,
    StdDatasetValidator,
};

#[test]
fn filesystem_dataset_infra_stages_manifests_catalogs_indexes_content_and_diff() {
    let root = unique_path("dataset-infra");
    let source = root.join("source");
    fs::create_dir_all(&source).expect("source");
    fs::write(
        source.join("train.jsonl"),
        r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}"#,
    )
    .expect("train");
    fs::write(
        source.join("valid.jsonl"),
        r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Bye"},{"role":"assistant","content":"Goodbye"}]}"#,
    )
    .expect("valid");

    let layout = DatasetStoreLayout::from_datasets_dir(root.join("datasets"));
    let initializer = StdDatasetStoreLayoutInitializer;
    initializer
        .ensure_dataset_store_layout(&layout)
        .expect("layout");

    let stager = StdDatasetSourceStager;
    let staged = stager
        .create_staging_source(&layout, "add")
        .expect("stage source");
    stager
        .copy_local_source(&source, &staged)
        .expect("copy local source");

    let manifest_builder = StdDatasetManifestBuilder;
    let manifest = manifest_builder
        .build_manifest(&staged.source_dir)
        .expect("manifest");
    assert_eq!(manifest.file_count(), 2);

    let package = StdDatasetPackageDetector
        .detect_package(&staged.source_dir, &manifest)
        .expect("package");
    assert!(package.tuning_ready);
    assert_eq!(package.splits.train.as_deref(), Some("train.jsonl"));

    let dataset_ref = StdDatasetIdentityGenerator
        .dataset_ref_for_manifest(&manifest)
        .expect("dataset ref");
    let metadata = dataset_metadata(dataset_ref.clone(), &source, package, &manifest);

    let content = FileDatasetContentStore;
    assert!(!content
        .dataset_content_exists(&layout, &dataset_ref)
        .expect("exists"));
    assert_eq!(
        content
            .install_staged_source(&layout, &staged, &dataset_ref)
            .expect("install"),
        layout.source_dir(&dataset_ref)
    );
    assert!(content
        .dataset_content_exists(&layout, &dataset_ref)
        .expect("exists after install"));

    let catalog = FileDatasetCatalogStore;
    catalog
        .save_dataset_metadata(&layout, &metadata)
        .expect("metadata");
    catalog
        .save_dataset_manifest(&layout, &dataset_ref, &manifest)
        .expect("manifest save");
    assert_eq!(catalog.list_datasets(&layout).expect("list").len(), 1);
    assert_eq!(
        catalog
            .inspect_dataset(
                &layout,
                &DatasetRefSelector::parse(dataset_ref.short_ref()).expect("selector"),
            )
            .expect("inspect")
            .metadata
            .dataset_ref,
        dataset_ref
    );

    let index = LocalDatasetSourceIndex {
        dataset_ref: dataset_ref.clone(),
        short_ref: dataset_ref.short_ref().to_string(),
        source_path: source.display().to_string(),
        imported_at: metadata.imported_at.clone(),
    };
    let index_store = FileDatasetSourceIndexStore;
    assert_eq!(
        index_store
            .save_local_source_index(&layout, &index)
            .expect("index"),
        layout.local_index_path(&dataset_ref)
    );

    let export_dir = root.join("export");
    assert_eq!(
        content
            .export_source(&layout, &dataset_ref, &export_dir)
            .expect("export"),
        export_dir
    );
    assert!(export_dir.join("train.jsonl").is_file());

    let diff = StdDatasetDiffer
        .diff_dataset(
            &layout,
            &DatasetRefSelector::parse(dataset_ref.short_ref()).expect("selector"),
            DatasetDiffTarget::LocalPath(export_dir),
        )
        .expect("diff");
    assert_eq!(diff.diff.summary.modified, 0);
    assert_eq!(diff.diff.summary.unchanged, 2);

    assert_eq!(
        index_store
            .remove_source_indexes(&layout, &dataset_ref)
            .expect("remove indexes"),
        vec![layout.local_index_path(&dataset_ref)]
    );
    content
        .remove_dataset_content(&layout, &dataset_ref)
        .expect("remove content");
    stager.discard_staging(&staged).expect("discard");
}

#[test]
fn validator_template_and_reference_guard_cover_local_dataset_workflows() {
    let root = unique_path("dataset-validator");
    let data = root.join("data");
    fs::create_dir_all(&data).expect("data");
    fs::write(
        data.join("train.jsonl"),
        r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}"#,
    )
    .expect("train");

    let validator = StdDatasetValidator;
    let outcome = validator
        .validate_dataset_path(&data)
        .expect("validate dataset");
    assert!(outcome.is_valid());
    assert!(outcome.tuning_ready);
    assert_eq!(outcome.record_count(), 1);

    let invalid = root.join("invalid.jsonl");
    fs::write(
        &invalid,
        r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"}]}"#,
    )
    .expect("invalid");
    let outcome = validator
        .validate_dataset_path(&invalid)
        .expect("validate invalid dataset");
    assert!(!outcome.is_valid());
    assert!(outcome
        .errors
        .iter()
        .any(|error| error.message.contains("final assistant")));

    let renderer = MarkdownDatasetTemplateRenderer;
    let template = renderer
        .render_template(&DatasetTemplateRequest::new(
            Some("support".to_string()),
            Some("zh-TW".to_string()),
        ))
        .expect("render template");
    let template_path = root.join("templates/generated.md");
    renderer
        .write_template(&template, &template_path)
        .expect("write template");
    assert!(fs::read_to_string(template_path)
        .expect("template body")
        .contains("Task/domain hint: `support`"));

    let dataset_ref = DatasetRef::parse("d".repeat(64)).expect("dataset ref");
    let layout = runtime_layout(&root.join("home"));
    let plan_dir = layout.train_dir.join("lora/plans/plan-a/runs/run-a");
    fs::create_dir_all(&plan_dir).expect("plan run dir");
    fs::write(
        layout.train_dir.join("lora/plans/plan-a/plan.toml"),
        format!("dataset_ref = \"{}\"\n", dataset_ref),
    )
    .expect("plan");
    fs::write(
        plan_dir.join("run.toml"),
        format!("dataset_ref = \"{}\"\n", dataset_ref),
    )
    .expect("run");

    let refs = FileDatasetReferenceGuard
        .train_refs_for_dataset(&layout, &dataset_ref)
        .expect("refs");
    assert_eq!(
        refs,
        vec!["plan:plan-a".to_string(), "run:run-a".to_string()]
    );
}

#[cfg(any())]
#[tokio::test]
async fn cloud_dataset_runtime_renders_prompt_without_python_entrypoint() {
    let root = unique_path("dataset-runtime");
    fs::create_dir_all(&root).expect("root");
    let runtime = python_runtime(&root.join("project"), &root.join("env"));
    let synth = CloudDatasetSynthRuntimeClient::new();
    let prompt = synth
        .render_synth_prompt(DatasetSynthPromptRuntimeRequest {
            runtime,
            request: DatasetSynthPromptRequest {
                prompt_source: DatasetPromptSource::Brief("make records".to_string()),
                split: DatasetSplitKind::Train,
                counts: DatasetSynthCounts {
                    count: Some(2),
                    ..DatasetSynthCounts::default()
                },
            },
        })
        .await
        .expect("prompt");
    assert!(prompt.contains("Tentgent Dataset Generation Template"));
    assert!(prompt.contains("make records"));
    assert!(prompt.contains("train=2"));
}

fn dataset_metadata(
    dataset_ref: DatasetRef,
    source: &Path,
    package: crate::features::dataset::domain::DatasetPackageMetadata,
    manifest: &crate::features::dataset::domain::DatasetManifest,
) -> DatasetMetadata {
    DatasetMetadata {
        short_ref: dataset_ref.short_ref().to_string(),
        dataset_ref,
        source_kind: DatasetSourceKind::Local,
        source_path: Some(source.display().to_string()),
        source_repo: None,
        source_revision: None,
        dataset_format: DatasetFormat::Directory,
        file_count: manifest.file_count(),
        total_bytes: manifest.total_bytes(),
        imported_at: "2026-05-17T00:00:00Z".to_string(),
        package,
    }
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

fn python_runtime(project: &Path, env: &Path) -> PythonRuntimeLayout {
    PythonRuntimeLayout {
        project_dir: project.to_path_buf(),
        env_dir: env.to_path_buf(),
        source: PythonRuntimeSource::DevelopmentSource,
    }
}

fn unique_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{label}-{nanos}"))
}
