#[cfg(any())]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::embedding::domain::{
    EmbeddingBackend, EmbeddingInput, EmbeddingResponse, EmbeddingRuntimeTarget,
};
#[cfg(any())]
use crate::features::embedding::domain::{EmbeddingRequest, ResolvedEmbeddingTarget};
use crate::features::embedding::infra::StdEmbeddingModelResolver;
use crate::features::embedding::ports::{
    EmbeddingModelResolveRequest, EmbeddingModelResolver, EmbeddingPortFuture,
    EmbeddingRuntimeClient, EmbeddingRuntimeRequest,
};
use crate::features::embedding::usecases::{
    EmbeddingPreparationRequest, EmbeddingUseCase, StdEmbeddingUseCase,
};
use crate::features::model::domain::{
    default_model_capability_source, ModelCapability, ModelCapabilityProofSource,
    ModelCapabilityProofStatus, ModelFormat, ModelInspection, ModelMetadata, ModelRef,
    ModelRefSelector, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::usecases::{
    ModelCatalogReadUseCase, ModelInspectRequest, ModelInspectResult, ModelListRequest,
    ModelListResult, ModelRuntimeExecutionEvidenceRecordRequest,
    ModelRuntimeExecutionEvidenceRecordResult, ModelRuntimeExecutionEvidenceRecorder,
};
#[cfg(any())]
use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeSource};
#[cfg(any())]
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::features::runtime::usecases::{
    RuntimeResolutionRequest, RuntimeResolutionResult, RuntimeResolutionUseCase,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

#[test]
fn embedding_input_rejects_empty_items() {
    let err = EmbeddingInput::new(Vec::new()).expect_err("empty input");
    assert_eq!(
        err.to_string(),
        "embedding input must contain at least one string"
    );

    let err = EmbeddingInput::new(vec![" ".to_string()]).expect_err("blank item");
    assert_eq!(err.to_string(), "embedding input strings must not be empty");
}

#[test]
fn std_embedding_model_resolver_accepts_embedding_safetensors_model() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Safetensors, vec![ModelCapability::Embedding]),
    };
    let resolver = StdEmbeddingModelResolver::new(&catalog);

    let result = resolver
        .resolve_embedding_model(EmbeddingModelResolveRequest {
            layout: layout_input(unique_path("embedding-model-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve embedding model");

    assert_eq!(result.model.metadata.model_ref, model_ref());
    assert_eq!(
        result.target,
        EmbeddingRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: EmbeddingBackend::TransformersPeft,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::Embedding],
        }
    );
}

#[test]
fn std_embedding_model_resolver_rejects_non_embedding_models() {
    for capability in [
        ModelCapability::Chat,
        ModelCapability::Rerank,
        ModelCapability::AudioTranscription,
        ModelCapability::VisionChat,
    ] {
        let catalog = FakeModelCatalog {
            metadata: model_metadata(ModelFormat::Safetensors, vec![capability]),
        };
        let resolver = StdEmbeddingModelResolver::new(&catalog);

        let err = resolver
            .resolve_embedding_model(EmbeddingModelResolveRequest {
                layout: layout_input(unique_path("embedding-non-embedding-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-embedding model");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err
            .to_string()
            .contains("requires model capability `embedding`"));
    }
}

#[test]
fn std_embedding_model_resolver_rejects_unsupported_embedding_format() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Gguf, vec![ModelCapability::Embedding]),
    };
    let resolver = StdEmbeddingModelResolver::new(&catalog);

    let err = resolver
        .resolve_embedding_model(EmbeddingModelResolveRequest {
            layout: layout_input(unique_path("embedding-gguf-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect_err("unsupported backend");

    assert!(matches!(err, KernelError::UnsupportedTarget(_)));
    assert!(err.to_string().contains("does not support `gguf`"));
}

#[tokio::test]
async fn std_embedding_usecase_records_failed_runtime_execution_proof() {
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Safetensors, vec![ModelCapability::Embedding]),
    };
    let model_resolver = StdEmbeddingModelResolver::new(&catalog);
    let runtime_client = FailingEmbeddingRuntimeClient;
    let evidence = RecordingRuntimeEvidenceRecorder::default();
    let usecase = StdEmbeddingUseCase::new_with_runtime_evidence(
        &runtime_resolution,
        &model_resolver,
        &runtime_client,
        &evidence,
    );

    let err = usecase
        .embed(embedding_preparation_request())
        .await
        .expect_err("embedding runtime failure");

    assert!(err.to_string().contains("embedding runtime failed"));
    let records = evidence.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].capability, ModelCapability::Embedding);
    assert_eq!(records[0].status, ModelCapabilityProofStatus::Failed);
    assert!(records[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("embedding runtime failed")));
}

#[cfg(any())]
#[cfg(unix)]
#[tokio::test]
async fn python_embedding_once_client_runs_entrypoint_with_embedding_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-embedding-once");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let entrypoint = root.join("tentgent-model-runtime-daemon");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > \"$TENTGENT_HOME/cwd.txt\"\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/args.txt\"\nprintf '{\"data\":[{\"index\":0,\"embedding\":[0.1,0.2]},{\"index\":1,\"embedding\":[0.3,0.4]}]}'\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver { entrypoint };
    let client = PythonEmbeddingOnceRuntimeClient::new(&executable_resolver);
    let response = client
        .embed(EmbeddingRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request: embedding_request(),
        })
        .await
        .expect("embed");

    assert_eq!(
        response,
        EmbeddingResponse {
            data: vec![
                EmbeddingVector {
                    index: 0,
                    embedding: vec![0.1, 0.2],
                },
                EmbeddingVector {
                    index: 1,
                    embedding: vec![0.3, 0.4],
                },
            ],
        }
    );
    let observed_cwd = PathBuf::from(
        fs::read_to_string(home.join("cwd.txt"))
            .expect("cwd")
            .trim(),
    );
    assert_eq!(
        fs::canonicalize(observed_cwd).expect("observed cwd"),
        fs::canonicalize(&project).expect("project")
    );
    let args = fs::read_to_string(home.join("args.txt")).expect("args");
    assert!(args.contains("--model-ref\n"));
    assert!(args.contains(&format!("{}\n", model_ref())));
    assert!(args.contains("--input\nfirst\n"));
    assert!(args.contains("--input\nsecond\n"));
}

struct FakeRuntimeResolutionUseCase;

impl RuntimeResolutionUseCase for FakeRuntimeResolutionUseCase {
    fn resolve_runtime(
        &self,
        request: RuntimeResolutionRequest,
    ) -> KernelResult<RuntimeResolutionResult> {
        let home = request
            .layout
            .home_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("/tmp/tentgent-embedding-usecase"));
        Ok(RuntimeResolutionResult {
            layout: runtime_layout(home),
            runtime: python_runtime(&home.join("project"), &home.join("env")),
        })
    }
}

struct FailingEmbeddingRuntimeClient;

impl EmbeddingRuntimeClient for FailingEmbeddingRuntimeClient {
    fn embed(
        &'_ self,
        _request: EmbeddingRuntimeRequest,
    ) -> EmbeddingPortFuture<'_, EmbeddingResponse> {
        Box::pin(async move {
            Err(KernelError::RuntimeStateUnavailable(
                "embedding runtime failed".to_string(),
            ))
        })
    }
}

#[derive(Default)]
struct RecordingRuntimeEvidenceRecorder {
    records: Mutex<Vec<ModelRuntimeExecutionEvidenceRecordRequest>>,
}

impl RecordingRuntimeEvidenceRecorder {
    fn records(&self) -> Vec<ModelRuntimeExecutionEvidenceRecordRequest> {
        self.records.lock().expect("records lock").clone()
    }
}

impl ModelRuntimeExecutionEvidenceRecorder for RecordingRuntimeEvidenceRecorder {
    fn record_runtime_execution_evidence(
        &self,
        request: ModelRuntimeExecutionEvidenceRecordRequest,
    ) -> KernelResult<ModelRuntimeExecutionEvidenceRecordResult> {
        let metadata = request.metadata.clone();
        self.records
            .lock()
            .expect("records lock")
            .push(request.clone());
        Ok(ModelRuntimeExecutionEvidenceRecordResult {
            proof: crate::features::model::domain::ModelCapabilityProof {
                model_ref: metadata.model_ref,
                capability: request.capability,
                status: request.status,
                source: ModelCapabilityProofSource::RuntimeExecution,
                primary_format: metadata.primary_format,
                mlx_runtime_family: metadata.mlx_runtime_family,
                backend: "safetensors".to_string(),
                runtime_version: None,
                runtime_profile: request.runtime_profile,
                runtime_profile_version: request.runtime_profile_version,
                server_ref: request.server_ref,
                checked_at: "2026-06-12T00:00:00Z".to_string(),
                error: request.error,
            },
        })
    }
}

#[derive(Clone)]
struct FakeModelCatalog {
    metadata: ModelMetadata,
}

impl ModelCatalogReadUseCase for FakeModelCatalog {
    fn list_models(&self, request: ModelListRequest) -> KernelResult<ModelListResult> {
        let layout = runtime_layout(
            request
                .layout
                .home_dir
                .as_deref()
                .unwrap_or(Path::new("/tmp")),
        );
        Ok(ModelListResult {
            store: ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
            layout,
            models: Vec::new(),
        })
    }

    fn inspect_model(&self, request: ModelInspectRequest) -> KernelResult<ModelInspectResult> {
        let layout = runtime_layout(
            request
                .layout
                .home_dir
                .as_deref()
                .unwrap_or(Path::new("/tmp")),
        );
        Ok(ModelInspectResult {
            store: ModelStoreLayout::from_models_dir(layout.models_dir.clone()),
            model: ModelInspection {
                metadata: self.metadata.clone(),
                store_path: layout.models_dir.join("store").join(model_ref().as_str()),
                manifest_path: layout
                    .models_dir
                    .join("store")
                    .join(model_ref().as_str())
                    .join("manifest.json"),
                variant_source_path: layout
                    .models_dir
                    .join("store")
                    .join(model_ref().as_str())
                    .join("source"),
            },
            layout,
        })
    }
}

#[cfg(any())]
struct FakeExecutableResolver {
    entrypoint: PathBuf,
}

#[cfg(any())]
impl RuntimeExecutableResolver for FakeExecutableResolver {
    fn python_binary_path(&self, _runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf> {
        Ok(PathBuf::from("python"))
    }

    fn entrypoint_path(
        &self,
        _runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf> {
        assert_eq!(entrypoint, RuntimeEntrypoint::ModelRuntimeDaemon);
        Ok(self.entrypoint.clone())
    }
}

#[cfg(any())]
fn embedding_request() -> EmbeddingRequest {
    EmbeddingRequest {
        target: ResolvedEmbeddingTarget {
            runtime: EmbeddingRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: EmbeddingBackend::TransformersPeft,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::Embedding],
            },
        },
        input: EmbeddingInput::new(vec!["first".to_string(), "second".to_string()])
            .expect("embedding input"),
    }
}

fn embedding_preparation_request() -> EmbeddingPreparationRequest {
    EmbeddingPreparationRequest {
        layout: layout_input(unique_path("embedding-usecase-home")),
        runtime: crate::features::runtime::domain::PythonRuntimeResolutionInput::default(),
        model_selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        input: EmbeddingInput::new(vec!["first".to_string()]).expect("embedding input"),
    }
}

fn model_metadata(format: ModelFormat, capabilities: Vec<ModelCapability>) -> ModelMetadata {
    ModelMetadata {
        model_ref: model_ref(),
        short_ref: model_ref().short_ref().to_string(),
        source_kind: ModelSourceKind::HuggingFace,
        source_repo: Some("org/model".to_string()),
        source_revision: Some("main".to_string()),
        source_path: None,
        primary_format: format,
        detected_formats: vec![format],
        mlx_runtime_family: None,
        model_capabilities: capabilities,
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 10,
        imported_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn model_ref() -> ModelRef {
    ModelRef::parse("4".repeat(64)).expect("model ref")
}

fn layout_input(home: PathBuf) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(home),
        data_root_dir: None,
    }
}

fn runtime_layout(home: &Path) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: home.to_path_buf(),
        data_root_dir: home.join("data"),
        config_path: home.join("config.toml"),
        models_dir: home.join("models"),
        adapters_dir: home.join("adapters"),
        datasets_dir: home.join("datasets"),
        sessions_dir: home.join("sessions"),
        servers_dir: home.join("servers"),
        train_dir: home.join("training"),
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
    }
}

fn python_runtime(project: &Path, env: &Path) -> PythonRuntimeLayout {
    PythonRuntimeLayout {
        project_dir: project.to_path_buf(),
        env_dir: env.to_path_buf(),
        source: PythonRuntimeSource::DevelopmentSource,
    }
}

fn unique_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{prefix}-{nanos}"))
}
