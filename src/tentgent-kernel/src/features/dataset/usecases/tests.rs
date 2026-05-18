use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::features::auth::domain::{
    AuthEnvLoadPolicy, AuthSecretMaterial, AuthSecretSource, Provider,
};
use crate::features::auth::usecases::{
    AuthSecretResolution, AuthSecretResolutionRequest, AuthSecretResolverUseCase,
};
use crate::features::dataset::domain::{
    DatasetDiffOutcome, DatasetEvalSplit, DatasetFormat, DatasetImportOutcome, DatasetInspection,
    DatasetMetadata, DatasetPackageMetadata, DatasetProvider, DatasetRef, DatasetRefSelector,
    DatasetRemovalOutcome, DatasetRenderedTemplate, DatasetSourceKind, DatasetSplitKind,
    DatasetStoreLayout, DatasetSummary, DatasetSynthCounts, DatasetSynthPromptRequest,
    DatasetSynthRequest, DatasetSynthRuntimeOutput, DatasetTemplateRequest,
    DatasetValidationOutcome,
};
use crate::features::dataset::infra::{
    FileDatasetCatalogStore, FileDatasetContentStore, FileDatasetReferenceGuard,
    FileDatasetSourceIndexStore, MarkdownDatasetTemplateRenderer, StdDatasetDiffer,
    StdDatasetIdentityGenerator, StdDatasetManifestBuilder, StdDatasetPackageDetector,
    StdDatasetSourceStager, StdDatasetStoreLayoutInitializer, StdDatasetValidator,
};
use crate::features::dataset::ports::{
    DatasetEvalRuntimeClient, DatasetEvalRuntimeRequest, DatasetPortFuture,
    DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeClient, DatasetSynthRuntimeRequest,
};
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeResolutionInput, PythonRuntimeSource,
};
use crate::features::runtime::usecases::{
    RuntimeResolutionRequest, RuntimeResolutionResult, RuntimeResolutionUseCase,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
};

use super::port::{
    DatasetCatalogReadUseCase, DatasetDiffRequest, DatasetDiffRightSelection, DatasetDiffUseCase,
    DatasetEvaluateRequest, DatasetEvaluationInputSelection, DatasetEvaluationUseCase,
    DatasetExportRequest, DatasetExportUseCase, DatasetInspectRequest, DatasetListRequest,
    DatasetLocalImportRequest, DatasetLocalImportUseCase, DatasetRemoveRequest,
    DatasetRemoveUseCase, DatasetSynthPromptRenderRequest, DatasetSynthesisUseCase,
    DatasetSynthesizeRequest, DatasetTemplateRenderRequest, DatasetTemplateUseCase,
    DatasetValidateRequest, DatasetValidationTargetSelection, DatasetValidationUseCase,
};
use super::{
    StdDatasetCatalogReadUseCase, StdDatasetDiffUseCase, StdDatasetEvaluationUseCase,
    StdDatasetExportUseCase, StdDatasetLocalImportUseCase, StdDatasetRemoveUseCase,
    StdDatasetSynthesisUseCase, StdDatasetTemplateUseCase, StdDatasetValidationUseCase,
};

#[tokio::test]
async fn dataset_usecase_ports_cover_dataset_workflows() {
    let usecases = FakeDatasetUseCases;
    let layout = layout_input("/tmp/tentgent-dataset-usecases");
    let selector = DatasetRefSelector::parse(dataset_ref().short_ref()).expect("selector");

    let listed = usecases
        .list_datasets(DatasetListRequest {
            layout: layout.clone(),
        })
        .expect("list datasets");
    assert_eq!(listed.datasets.len(), 1);
    assert_eq!(listed.store.datasets_dir, listed.layout.datasets_dir);

    let inspected = usecases
        .inspect_dataset(DatasetInspectRequest {
            layout: layout.clone(),
            selector: selector.clone(),
        })
        .expect("inspect dataset");
    assert_eq!(inspected.dataset.metadata.dataset_ref, dataset_ref());

    let imported = usecases
        .import_local_dataset(DatasetLocalImportRequest {
            layout: layout.clone(),
            source_path: PathBuf::from("/tmp/dataset"),
        })
        .expect("import dataset");
    assert!(!imported.outcome.deduplicated);

    let validated = usecases
        .validate_dataset(DatasetValidateRequest {
            layout: layout.clone(),
            target: DatasetValidationTargetSelection::ManagedDataset(selector.clone()),
        })
        .expect("validate dataset");
    assert!(validated.outcome.is_valid());

    let templated = usecases
        .render_dataset_template(DatasetTemplateRenderRequest {
            template: DatasetTemplateRequest::new(Some("chat".into()), Some("zh-TW".into())),
            output_path: None,
        })
        .expect("template");
    assert_eq!(
        templated.rendered.template_version,
        "tentgent.dataset.synth.v1"
    );

    let prompt = usecases
        .render_synth_prompt(DatasetSynthPromptRenderRequest {
            layout: layout.clone(),
            runtime: runtime_input(),
            prompt: synth_prompt_request(),
        })
        .await
        .expect("render synth prompt");
    assert!(prompt.prompt.contains("prompt"));

    let synthesized = usecases
        .synthesize_dataset(DatasetSynthesizeRequest {
            layout: layout.clone(),
            runtime: runtime_input(),
            auth: AuthSecretResolutionRequest::for_secret_use(
                Provider::OpenAI,
                AuthEnvLoadPolicy::ProcessOnly,
            )
            .with_request_secret("test-openai-key"),
            synth: synth_request(PathBuf::from("/tmp/generated")),
        })
        .await
        .expect("synthesize");
    assert_eq!(synthesized.output.outcome["status"], "ok");

    let evaluated = usecases
        .evaluate_dataset(DatasetEvaluateRequest {
            layout: layout.clone(),
            runtime: runtime_input(),
            auth: AuthSecretResolutionRequest::for_secret_use(
                Provider::OpenAI,
                AuthEnvLoadPolicy::ProcessOnly,
            )
            .with_request_secret("test-openai-key"),
            provider: DatasetProvider::OpenAI,
            provider_model: "gpt-test".to_string(),
            input: DatasetEvaluationInputSelection::ManagedDataset(selector.clone()),
            output_dir: PathBuf::from("/tmp/report"),
            split: DatasetEvalSplit::Train,
            max_records: 20,
            criteria: None,
            max_tokens: None,
            temperature: 0.0,
            timeout_seconds: 180.0,
        })
        .await
        .expect("evaluate");
    assert_eq!(evaluated.report["status"], "reviewed");

    let exported = usecases
        .export_dataset(DatasetExportRequest {
            layout: layout.clone(),
            selector: selector.clone(),
            destination_path: PathBuf::from("/tmp/export"),
        })
        .expect("export");
    assert_eq!(exported.outcome.metadata.dataset_ref, dataset_ref());

    let diffed = usecases
        .diff_dataset(DatasetDiffRequest {
            layout: layout.clone(),
            left: selector.clone(),
            right: DatasetDiffRightSelection::ManagedDataset(selector.clone()),
        })
        .expect("diff");
    assert_eq!(diffed.outcome.diff.summary.modified, 0);

    let removed = usecases
        .remove_dataset(DatasetRemoveRequest { layout, selector })
        .expect("remove");
    assert_eq!(removed.outcome.metadata.dataset_ref, dataset_ref());
}

#[test]
fn standard_dataset_usecases_import_list_validate_export_diff_and_remove_local_dataset() {
    let home = unique_path("dataset-local-usecase");
    let source_dir = home.join("source");
    fs::create_dir_all(&source_dir).expect("source dir");
    fs::write(source_dir.join("train.jsonl"), sample_record_line()).expect("train jsonl");

    let layout_resolver = FakeLayoutResolver;
    let initializer = StdDatasetStoreLayoutInitializer;
    let stager = StdDatasetSourceStager;
    let manifest_builder = StdDatasetManifestBuilder;
    let identity = StdDatasetIdentityGenerator;
    let package_detector = StdDatasetPackageDetector;
    let catalog = FileDatasetCatalogStore;
    let indexes = FileDatasetSourceIndexStore;
    let content = FileDatasetContentStore;
    let validator = StdDatasetValidator;
    let differ = StdDatasetDiffer;
    let reference_guard = FileDatasetReferenceGuard;

    let importer = StdDatasetLocalImportUseCase::new(
        &layout_resolver,
        &initializer,
        &stager,
        &manifest_builder,
        &identity,
        &package_detector,
        &catalog,
        &indexes,
        &content,
    );
    let imported = importer
        .import_local_dataset(DatasetLocalImportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            source_path: source_dir.clone(),
        })
        .expect("import dataset");
    assert!(!imported.outcome.deduplicated);
    assert_eq!(
        imported.outcome.metadata.dataset_format,
        DatasetFormat::Directory
    );
    assert!(imported.outcome.metadata.package.tuning_ready);
    assert!(imported.outcome.store_path.is_dir());

    let reader = StdDatasetCatalogReadUseCase::new(&layout_resolver, &catalog);
    let listed = reader
        .list_datasets(DatasetListRequest {
            layout: layout_input(home.to_str().expect("home path")),
        })
        .expect("list datasets");
    assert_eq!(listed.datasets.len(), 1);

    let selector =
        DatasetRefSelector::parse(imported.outcome.metadata.short_ref.as_str()).expect("selector");
    let inspected = reader
        .inspect_dataset(DatasetInspectRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
        })
        .expect("inspect dataset");
    assert_eq!(
        inspected.dataset.metadata.dataset_ref,
        imported.outcome.metadata.dataset_ref
    );

    let validation = StdDatasetValidationUseCase::new(&layout_resolver, &catalog, &validator)
        .validate_dataset(DatasetValidateRequest {
            layout: layout_input(home.to_str().expect("home path")),
            target: DatasetValidationTargetSelection::ManagedDataset(selector.clone()),
        })
        .expect("validate dataset");
    assert!(validation.outcome.is_valid());
    assert_eq!(validation.outcome.record_count(), 1);

    let export_dir = home.join("export");
    let exported = StdDatasetExportUseCase::new(&layout_resolver, &catalog, &content)
        .export_dataset(DatasetExportRequest {
            layout: layout_input(home.to_str().expect("home path")),
            selector: selector.clone(),
            destination_path: export_dir.clone(),
        })
        .expect("export dataset");
    assert_eq!(exported.outcome.destination_path, export_dir);
    assert!(exported
        .outcome
        .destination_path
        .join("train.jsonl")
        .is_file());

    let diffed = StdDatasetDiffUseCase::new(&layout_resolver, &differ)
        .diff_dataset(DatasetDiffRequest {
            layout: layout_input(home.to_str().expect("home path")),
            left: selector.clone(),
            right: DatasetDiffRightSelection::LocalPath(exported.outcome.destination_path.clone()),
        })
        .expect("diff dataset");
    assert_eq!(diffed.outcome.diff.summary.modified, 0);
    assert_eq!(diffed.outcome.diff.summary.added, 0);
    assert_eq!(diffed.outcome.diff.summary.removed, 0);

    let removed = StdDatasetRemoveUseCase::new(
        &layout_resolver,
        &catalog,
        &indexes,
        &content,
        &reference_guard,
    )
    .remove_dataset(DatasetRemoveRequest {
        layout: layout_input(home.to_str().expect("home path")),
        selector,
    })
    .expect("remove dataset");
    assert_eq!(
        removed.outcome.metadata.dataset_ref,
        imported.outcome.metadata.dataset_ref
    );
    assert!(!removed.outcome.store_path.exists());
}

#[tokio::test]
async fn standard_dataset_runtime_usecases_resolve_runtime_auth_and_clients() {
    let home = unique_path("dataset-runtime-usecase");
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let auth_resolver = FakeAuthSecretResolver;
    let synth_client = FakeDatasetSynthRuntimeClient;
    let eval_client = FakeDatasetEvalRuntimeClient;
    let catalog = FileDatasetCatalogStore;

    let synthesis =
        StdDatasetSynthesisUseCase::new(&runtime_resolution, &auth_resolver, &synth_client);
    let prompt = synthesis
        .render_synth_prompt(DatasetSynthPromptRenderRequest {
            layout: layout_input(home.to_str().expect("home path")),
            runtime: runtime_input(),
            prompt: synth_prompt_request(),
        })
        .await
        .expect("render synth prompt");
    assert_eq!(
        prompt.runtime.project_dir,
        PathBuf::from("/tmp/python-project")
    );
    assert!(prompt.prompt.contains("train"));

    let synthesized = synthesis
        .synthesize_dataset(DatasetSynthesizeRequest {
            layout: layout_input(home.to_str().expect("home path")),
            runtime: runtime_input(),
            auth: AuthSecretResolutionRequest::for_secret_use(
                Provider::OpenAI,
                AuthEnvLoadPolicy::ProcessOnly,
            ),
            synth: synth_request(home.join("generated")),
        })
        .await
        .expect("synthesize dataset");
    assert_eq!(synthesized.output.outcome["provider"], "openai");

    let evaluation = StdDatasetEvaluationUseCase::new(
        &runtime_resolution,
        &auth_resolver,
        &catalog,
        &eval_client,
    );
    let evaluated = evaluation
        .evaluate_dataset(DatasetEvaluateRequest {
            layout: layout_input(home.to_str().expect("home path")),
            runtime: runtime_input(),
            auth: AuthSecretResolutionRequest::for_secret_use(
                Provider::OpenAI,
                AuthEnvLoadPolicy::ProcessOnly,
            ),
            provider: DatasetProvider::OpenAI,
            provider_model: "gpt-test".to_string(),
            input: DatasetEvaluationInputSelection::LocalPath(home.join("generated")),
            output_dir: home.join("report"),
            split: DatasetEvalSplit::Train,
            max_records: 5,
            criteria: Some("quality".to_string()),
            max_tokens: Some(1000),
            temperature: 0.0,
            timeout_seconds: 60.0,
        })
        .await
        .expect("evaluate dataset");
    assert_eq!(evaluated.report["status"], "reviewed");
    assert_eq!(evaluated.input_path, home.join("generated"));
}

#[test]
fn standard_dataset_template_usecase_writes_template_when_requested() {
    let home = unique_path("dataset-template-usecase");
    let output = home.join("dataset-template.md");
    let renderer = MarkdownDatasetTemplateRenderer;
    let usecase = StdDatasetTemplateUseCase::new(&renderer);

    let rendered = usecase
        .render_dataset_template(DatasetTemplateRenderRequest {
            template: DatasetTemplateRequest::new(
                Some("support".to_string()),
                Some("zh-TW".to_string()),
            ),
            output_path: Some(output.clone()),
        })
        .expect("render template");

    assert_eq!(rendered.output_path, Some(output.clone()));
    assert!(output.is_file());
    assert!(rendered.rendered.body.contains("support"));
    assert!(rendered.rendered.body.contains("zh-TW"));
}

struct FakeDatasetUseCases;

impl DatasetCatalogReadUseCase for FakeDatasetUseCases {
    fn list_datasets(
        &self,
        request: DatasetListRequest,
    ) -> KernelResult<super::port::DatasetListResult> {
        let layout = runtime_layout_from_input(&request.layout);
        Ok(super::port::DatasetListResult {
            store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
            layout,
            datasets: vec![DatasetSummary {
                metadata: dataset_metadata(),
                store_path: PathBuf::from("/tmp/store"),
            }],
        })
    }

    fn inspect_dataset(
        &self,
        request: DatasetInspectRequest,
    ) -> KernelResult<super::port::DatasetInspectResult> {
        let layout = runtime_layout_from_input(&request.layout);
        Ok(super::port::DatasetInspectResult {
            store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
            layout,
            dataset: dataset_inspection(),
        })
    }
}

impl DatasetLocalImportUseCase for FakeDatasetUseCases {
    fn import_local_dataset(
        &self,
        request: DatasetLocalImportRequest,
    ) -> KernelResult<super::port::DatasetLocalImportResult> {
        let layout = runtime_layout_from_input(&request.layout);
        Ok(super::port::DatasetLocalImportResult {
            store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
            layout,
            outcome: DatasetImportOutcome {
                metadata: dataset_metadata(),
                store_path: PathBuf::from("/tmp/store"),
                source_index_path: PathBuf::from("/tmp/index.toml"),
                deduplicated: false,
            },
        })
    }
}

impl DatasetValidationUseCase for FakeDatasetUseCases {
    fn validate_dataset(
        &self,
        request: DatasetValidateRequest,
    ) -> KernelResult<super::port::DatasetValidateResult> {
        let layout = runtime_layout_from_input(&request.layout);
        Ok(super::port::DatasetValidateResult {
            store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
            layout,
            dataset: Some(dataset_inspection()),
            outcome: DatasetValidationOutcome {
                path: PathBuf::from("/tmp/source"),
                target_kind:
                    crate::features::dataset::domain::DatasetValidationTargetKind::Directory,
                tuning_ready: true,
                splits: Vec::new(),
                warnings: Vec::new(),
                errors: Vec::new(),
            },
        })
    }
}

impl DatasetTemplateUseCase for FakeDatasetUseCases {
    fn render_dataset_template(
        &self,
        _request: DatasetTemplateRenderRequest,
    ) -> KernelResult<super::port::DatasetTemplateRenderResult> {
        Ok(super::port::DatasetTemplateRenderResult {
            rendered: DatasetRenderedTemplate {
                template_version: "tentgent.dataset.synth.v1".to_string(),
                body: "body".to_string(),
            },
            output_path: None,
        })
    }
}

impl DatasetSynthesisUseCase for FakeDatasetUseCases {
    fn render_synth_prompt(
        &self,
        request: DatasetSynthPromptRenderRequest,
    ) -> super::port::DatasetUseCaseFuture<'_, super::port::DatasetSynthPromptRenderResult> {
        Box::pin(async move {
            let layout = runtime_layout_from_input(&request.layout);
            Ok(super::port::DatasetSynthPromptRenderResult {
                layout,
                runtime: runtime_layout(),
                prompt: "prompt".to_string(),
            })
        })
    }

    fn synthesize_dataset(
        &self,
        request: DatasetSynthesizeRequest,
    ) -> super::port::DatasetUseCaseFuture<'_, super::port::DatasetSynthesizeResult> {
        Box::pin(async move {
            let layout = runtime_layout_from_input(&request.layout);
            Ok(super::port::DatasetSynthesizeResult {
                layout,
                runtime: runtime_layout(),
                output: DatasetSynthRuntimeOutput {
                    outcome: json!({"status":"ok"}),
                    progress_events: vec![json!({"type":"progress"})],
                    progress_truncated: false,
                },
            })
        })
    }
}

impl DatasetEvaluationUseCase for FakeDatasetUseCases {
    fn evaluate_dataset(
        &self,
        request: DatasetEvaluateRequest,
    ) -> super::port::DatasetUseCaseFuture<'_, super::port::DatasetEvaluateResult> {
        Box::pin(async move {
            let layout = runtime_layout_from_input(&request.layout);
            Ok(super::port::DatasetEvaluateResult {
                store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
                layout,
                runtime: runtime_layout(),
                dataset: Some(dataset_inspection()),
                input_path: PathBuf::from("/tmp/source"),
                report: json!({"status":"reviewed"}),
            })
        })
    }
}

impl DatasetExportUseCase for FakeDatasetUseCases {
    fn export_dataset(
        &self,
        request: DatasetExportRequest,
    ) -> KernelResult<super::port::DatasetExportResult> {
        let layout = runtime_layout_from_input(&request.layout);
        Ok(super::port::DatasetExportResult {
            store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
            layout,
            outcome: crate::features::dataset::domain::DatasetExportOutcome {
                metadata: dataset_metadata(),
                managed_source_path: PathBuf::from("/tmp/source"),
                destination_path: request.destination_path,
            },
        })
    }
}

impl DatasetDiffUseCase for FakeDatasetUseCases {
    fn diff_dataset(
        &self,
        request: DatasetDiffRequest,
    ) -> KernelResult<super::port::DatasetDiffResult> {
        let layout = runtime_layout_from_input(&request.layout);
        Ok(super::port::DatasetDiffResult {
            store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
            layout,
            outcome: DatasetDiffOutcome {
                left: crate::features::dataset::domain::DatasetDiffSide {
                    label: "left".to_string(),
                    short_ref: Some("left".to_string()),
                    tuning_ready: true,
                    splits: "train".to_string(),
                    path: None,
                },
                right: crate::features::dataset::domain::DatasetDiffSide {
                    label: "right".to_string(),
                    short_ref: Some("right".to_string()),
                    tuning_ready: true,
                    splits: "train".to_string(),
                    path: None,
                },
                diff: crate::features::dataset::domain::DatasetManifestDiff {
                    summary: crate::features::dataset::domain::DatasetDiffSummary::default(),
                    files: Vec::new(),
                },
            },
        })
    }
}

impl DatasetRemoveUseCase for FakeDatasetUseCases {
    fn remove_dataset(
        &self,
        request: DatasetRemoveRequest,
    ) -> KernelResult<super::port::DatasetRemoveResult> {
        let layout = runtime_layout_from_input(&request.layout);
        Ok(super::port::DatasetRemoveResult {
            store: DatasetStoreLayout::from_datasets_dir(layout.datasets_dir.clone()),
            layout,
            outcome: DatasetRemovalOutcome {
                metadata: dataset_metadata(),
                store_path: PathBuf::from("/tmp/store"),
                removed_index_paths: vec![PathBuf::from("/tmp/index.toml")],
                blockers: Vec::new(),
            },
        })
    }
}

struct FakeLayoutResolver;

impl RuntimeLayoutResolver for FakeLayoutResolver {
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout> {
        Ok(runtime_layout_from_input(&input))
    }
}

struct FakeRuntimeResolutionUseCase;

impl RuntimeResolutionUseCase for FakeRuntimeResolutionUseCase {
    fn resolve_runtime(
        &self,
        request: RuntimeResolutionRequest,
    ) -> KernelResult<RuntimeResolutionResult> {
        Ok(RuntimeResolutionResult {
            layout: runtime_layout_from_input(&request.layout),
            runtime: runtime_layout(),
        })
    }
}

struct FakeAuthSecretResolver;

impl AuthSecretResolverUseCase for FakeAuthSecretResolver {
    fn resolve_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution> {
        Ok(AuthSecretResolution {
            provider: request.provider,
            secret: Some(AuthSecretMaterial::new(
                request.provider,
                AuthSecretSource::Request,
                "test-secret",
            )),
            keychain_read_attempted: false,
        })
    }
}

struct FakeDatasetSynthRuntimeClient;

impl DatasetSynthRuntimeClient for FakeDatasetSynthRuntimeClient {
    fn render_synth_prompt(
        &self,
        request: DatasetSynthPromptRuntimeRequest,
    ) -> DatasetPortFuture<'_, String> {
        Box::pin(async move { Ok(format!("prompt for {}", request.request.split.as_str())) })
    }

    fn synthesize_dataset(
        &self,
        request: DatasetSynthRuntimeRequest,
    ) -> DatasetPortFuture<'_, DatasetSynthRuntimeOutput> {
        Box::pin(async move {
            Ok(DatasetSynthRuntimeOutput {
                outcome: json!({
                    "provider": request.request.provider.as_str(),
                    "model": request.request.provider_model,
                    "status": "ok"
                }),
                progress_events: vec![json!({"type":"progress","split":"train"})],
                progress_truncated: false,
            })
        })
    }
}

struct FakeDatasetEvalRuntimeClient;

impl DatasetEvalRuntimeClient for FakeDatasetEvalRuntimeClient {
    fn evaluate_dataset(
        &self,
        request: DatasetEvalRuntimeRequest,
    ) -> DatasetPortFuture<'_, serde_json::Value> {
        Box::pin(async move {
            Ok(json!({
                "status": "reviewed",
                "provider": request.request.provider.as_str(),
                "model": request.request.provider_model
            }))
        })
    }

    fn runtime_debug(
        &self,
        _error_detail: &str,
    ) -> Option<crate::features::dataset::domain::DatasetRuntimeDebug> {
        None
    }
}

fn synth_prompt_request() -> DatasetSynthPromptRequest {
    DatasetSynthPromptRequest {
        prompt_source: crate::features::dataset::domain::DatasetPromptSource::Brief(
            "make data".to_string(),
        ),
        split: DatasetSplitKind::Train,
        counts: DatasetSynthCounts {
            count: Some(2),
            ..DatasetSynthCounts::default()
        },
    }
}

fn synth_request(output_dir: PathBuf) -> DatasetSynthRequest {
    DatasetSynthRequest {
        provider: DatasetProvider::OpenAI,
        provider_model: "gpt-test".to_string(),
        output_dir,
        prompt_source: crate::features::dataset::domain::DatasetPromptSource::Brief(
            "make data".to_string(),
        ),
        split: DatasetSplitKind::Train,
        counts: DatasetSynthCounts {
            count: Some(2),
            ..DatasetSynthCounts::default()
        },
        max_tokens: Some(1000),
        temperature: 0.0,
        timeout_seconds: 60.0,
        retries: 1,
    }
}

fn dataset_inspection() -> DatasetInspection {
    DatasetInspection {
        metadata: dataset_metadata(),
        store_path: PathBuf::from("/tmp/store"),
        manifest_path: PathBuf::from("/tmp/manifest.json"),
        source_path: PathBuf::from("/tmp/source"),
    }
}

fn dataset_metadata() -> DatasetMetadata {
    let dataset_ref = dataset_ref();
    DatasetMetadata {
        short_ref: dataset_ref.short_ref().to_string(),
        dataset_ref,
        source_kind: DatasetSourceKind::Local,
        source_path: Some("/tmp/source".to_string()),
        source_repo: None,
        source_revision: None,
        dataset_format: DatasetFormat::Directory,
        file_count: 1,
        total_bytes: 100,
        imported_at: "2026-05-17T00:00:00Z".to_string(),
        package: DatasetPackageMetadata {
            tuning_ready: true,
            splits: crate::features::dataset::domain::DatasetSplits {
                train: Some("train.jsonl".to_string()),
                ..Default::default()
            },
            warnings: Vec::new(),
        },
    }
}

fn dataset_ref() -> DatasetRef {
    DatasetRef::parse("d".repeat(64)).expect("dataset ref")
}

fn layout_input(home: impl AsRef<Path>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::Create,
        home_dir: Some(home.as_ref().to_path_buf()),
        data_root_dir: None,
    }
}

fn runtime_input() -> PythonRuntimeResolutionInput {
    PythonRuntimeResolutionInput {
        project_dir: Some(PathBuf::from("/tmp/python-project")),
        python_env_dir: Some(PathBuf::from("/tmp/python-env")),
    }
}

fn runtime_layout_from_input(input: &RuntimeLayoutInput) -> RuntimeLayout {
    let home = input
        .home_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("/tmp/tentgent-home"));
    RuntimeLayout {
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
        python_env_dir: home.join("runtime/python"),
        bootstrap_dir: home.join("bootstrap"),
        bootstrap_uv_dir: home.join("bootstrap/uv"),
        bootstrap_uv_cache_dir: home.join("bootstrap/uv-cache"),
        capabilities_path: home.join("runtime/capabilities.toml"),
        auth_metadata_path: home.join("runtime/auth.toml"),
        data_root_dir: home.clone(),
        home_dir: home,
    }
}

fn runtime_layout() -> PythonRuntimeLayout {
    PythonRuntimeLayout {
        project_dir: PathBuf::from("/tmp/python-project"),
        env_dir: PathBuf::from("/tmp/python-env"),
        source: PythonRuntimeSource::EnvironmentOverride,
    }
}

fn sample_record_line() -> &'static str {
    r#"{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi"}]}"#
}

fn unique_path(label: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("tentgent-{label}-{}-{millis}", std::process::id()))
}
