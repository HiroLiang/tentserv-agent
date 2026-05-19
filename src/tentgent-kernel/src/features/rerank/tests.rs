use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::model::domain::{
    default_model_capability_source, ModelCapability, ModelFormat, ModelInspection, ModelMetadata,
    ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::usecases::{
    ModelCatalogReadUseCase, ModelInspectRequest, ModelInspectResult, ModelListRequest,
    ModelListResult,
};
use crate::features::rerank::domain::{
    RerankBackend, RerankInput, RerankRequest, RerankResponse, RerankRuntimeTarget, RerankScore,
    ResolvedRerankTarget,
};
use crate::features::rerank::infra::{PythonRerankOnceRuntimeClient, StdRerankModelResolver};
use crate::features::rerank::ports::{
    RerankModelResolveRequest, RerankModelResolver, RerankRuntimeClient, RerankRuntimeRequest,
};
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint,
};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

#[test]
fn rerank_input_rejects_invalid_values() {
    let err =
        RerankInput::new(" ".to_string(), vec!["doc".to_string()], None).expect_err("blank query");
    assert_eq!(err.to_string(), "rerank query must not be empty");

    let err = RerankInput::new("query".to_string(), Vec::new(), None).expect_err("empty documents");
    assert_eq!(
        err.to_string(),
        "rerank documents must contain at least one string"
    );

    let err = RerankInput::new("query".to_string(), vec![" ".to_string()], None)
        .expect_err("blank document");
    assert_eq!(err.to_string(), "rerank documents must not be empty");

    let err = RerankInput::new("query".to_string(), vec!["doc".to_string()], Some(2))
        .expect_err("invalid top_n");
    assert!(err.to_string().contains("top_n must be between 1"));
}

#[test]
fn rerank_response_sorts_scores_and_preserves_original_indexes() {
    let response = RerankResponse::ranked_from_scores(vec![0.2, 0.9, 0.9, 0.1], Some(3));

    assert_eq!(
        response,
        RerankResponse {
            data: vec![
                RerankScore {
                    index: 1,
                    score: 0.9,
                },
                RerankScore {
                    index: 2,
                    score: 0.9,
                },
                RerankScore {
                    index: 0,
                    score: 0.2,
                },
            ],
        }
    );
}

#[test]
fn std_rerank_model_resolver_accepts_rerank_safetensors_model() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Safetensors, vec![ModelCapability::Rerank]),
    };
    let resolver = StdRerankModelResolver::new(&catalog);

    let result = resolver
        .resolve_rerank_model(RerankModelResolveRequest {
            layout: layout_input(unique_path("rerank-model-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve rerank model");

    assert_eq!(result.model.metadata.model_ref, model_ref());
    assert_eq!(
        result.target,
        RerankRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: RerankBackend::TransformersSequenceClassification,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::Rerank],
        }
    );
}

#[test]
fn std_rerank_model_resolver_rejects_non_rerank_models() {
    for capability in [ModelCapability::Chat, ModelCapability::Embedding] {
        let catalog = FakeModelCatalog {
            metadata: model_metadata(ModelFormat::Safetensors, vec![capability]),
        };
        let resolver = StdRerankModelResolver::new(&catalog);

        let err = resolver
            .resolve_rerank_model(RerankModelResolveRequest {
                layout: layout_input(unique_path("rerank-non-rerank-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-rerank model");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err
            .to_string()
            .contains("requires model capability `rerank`"));
    }
}

#[test]
fn std_rerank_model_resolver_rejects_unsupported_rerank_format() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Mlx, vec![ModelCapability::Rerank]),
    };
    let resolver = StdRerankModelResolver::new(&catalog);

    let err = resolver
        .resolve_rerank_model(RerankModelResolveRequest {
            layout: layout_input(unique_path("rerank-mlx-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect_err("unsupported backend");

    assert!(matches!(err, KernelError::UnsupportedTarget(_)));
    assert!(err.to_string().contains("does not support `mlx`"));
}

#[cfg(unix)]
#[tokio::test]
async fn python_rerank_once_client_runs_entrypoint_with_rerank_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-rerank-once");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let entrypoint = root.join("tentgent-rerank-once");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > \"$TENTGENT_HOME/cwd.txt\"\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/args.txt\"\nprintf '{\"data\":[{\"index\":1,\"score\":0.9},{\"index\":0,\"score\":0.2}]}'\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver { entrypoint };
    let client = PythonRerankOnceRuntimeClient::new(&executable_resolver);
    let response = client
        .rerank(RerankRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request: rerank_request(),
        })
        .await
        .expect("rerank");

    assert_eq!(
        response,
        RerankResponse {
            data: vec![
                RerankScore {
                    index: 1,
                    score: 0.9,
                },
                RerankScore {
                    index: 0,
                    score: 0.2,
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
    assert!(args.contains("--query\nquestion\n"));
    assert!(args.contains("--document\nfirst\n"));
    assert!(args.contains("--document\nsecond\n"));
    assert!(args.contains("--top-n\n1\n"));
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

struct FakeExecutableResolver {
    entrypoint: PathBuf,
}

impl RuntimeExecutableResolver for FakeExecutableResolver {
    fn python_binary_path(&self, _runtime: &PythonRuntimeLayout) -> KernelResult<PathBuf> {
        Ok(PathBuf::from("python"))
    }

    fn entrypoint_path(
        &self,
        _runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<PathBuf> {
        assert_eq!(entrypoint, RuntimeEntrypoint::RerankOnce);
        Ok(self.entrypoint.clone())
    }
}

fn rerank_request() -> RerankRequest {
    RerankRequest {
        target: ResolvedRerankTarget {
            runtime: RerankRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: RerankBackend::TransformersSequenceClassification,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::Rerank],
            },
        },
        input: RerankInput::new(
            "question".to_string(),
            vec!["first".to_string(), "second".to_string()],
            Some(1),
        )
        .expect("rerank input"),
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
        model_capabilities: capabilities,
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 10,
        imported_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn model_ref() -> ModelRef {
    ModelRef::parse("5".repeat(64)).expect("model ref")
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
