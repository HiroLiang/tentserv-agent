use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::adapter::domain::{
    AdapterBackendSupport, AdapterFormat, AdapterInspection, AdapterMetadata, AdapterRef,
    AdapterRefSelector, AdapterSourceKind, AdapterStoreLayout, AdapterType,
};
use crate::features::adapter::usecases::{
    AdapterCompatibilityCheckRequest, AdapterCompatibilityCheckResult,
    AdapterCompatibilityCheckUseCase,
};
use crate::features::chat::domain::{
    ChatBackend, ChatFinishReason, ChatGenerationOptions, ChatMessage, ChatPrompt, ChatRequest,
    ChatRuntimeTarget, ChatStreamEvent, ResolvedChatAdapter, ResolvedChatTarget,
};
use crate::features::chat::ports::{
    ChatAdapterResolveRequest, ChatAdapterResolver, ChatModelResolveRequest, ChatModelResolver,
    ChatRuntimeClient, ChatRuntimeRequest,
};
use crate::features::model::domain::{
    default_model_capability_source, ModelCapability, ModelFormat, ModelInspection, ModelMetadata,
    ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
};
use crate::features::model::usecases::{
    ModelCatalogReadUseCase, ModelInspectRequest, ModelInspectResult, ModelListRequest,
    ModelListResult,
};
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint,
};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

use super::{StdChatAdapterResolver, StdChatModelResolver};

#[test]
fn std_chat_model_resolver_maps_model_metadata_to_local_chat_target() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Safetensors, vec![ModelCapability::Chat]),
    };
    let resolver = StdChatModelResolver::new(&catalog);

    let result = resolver
        .resolve_chat_model(ChatModelResolveRequest {
            layout: layout_input(unique_path("chat-model-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve model");

    assert_eq!(result.model.metadata.model_ref, model_ref());
    assert_eq!(
        result.target,
        ChatRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: ChatBackend::TransformersPeft,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::Chat],
        }
    );
}

#[test]
fn std_chat_model_resolver_rejects_non_chat_models() {
    for capability in [
        ModelCapability::Embedding,
        ModelCapability::Rerank,
        ModelCapability::AudioTranscription,
        ModelCapability::VisionChat,
    ] {
        let catalog = FakeModelCatalog {
            metadata: model_metadata(ModelFormat::Safetensors, vec![capability]),
        };
        let resolver = StdChatModelResolver::new(&catalog);

        let err = resolver
            .resolve_chat_model(ChatModelResolveRequest {
                layout: layout_input(unique_path("chat-model-non-chat-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-chat model");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
    }
}

#[test]
fn std_chat_adapter_resolver_maps_compatibility_result_to_chat_adapter() {
    let compatibility = FakeAdapterCompatibility {
        adapter: adapter_inspection(),
    };
    let resolver = StdChatAdapterResolver::new(&compatibility);
    let target = crate::features::adapter::domain::AdapterCompatibilityTarget {
        base_model_ref: model_ref(),
        base_model_source_repo: Some("org/model".to_string()),
        base_model_source_revision: Some("main".to_string()),
        base_model_capabilities: vec![ModelCapability::Chat],
        required_capability: ModelCapability::Chat,
        backend: AdapterBackendSupport::TransformersPeft,
    };

    let result = resolver
        .resolve_chat_adapter(ChatAdapterResolveRequest {
            layout: layout_input(unique_path("chat-adapter-home")),
            selector: AdapterRefSelector::parse(adapter_ref().short_ref()).expect("selector"),
            target,
        })
        .expect("resolve adapter");

    assert_eq!(result.adapter.metadata.adapter_ref, adapter_ref());
    assert_eq!(
        result.target,
        ResolvedChatAdapter {
            adapter_ref: adapter_ref(),
            backend: AdapterBackendSupport::TransformersPeft,
            source_path: PathBuf::from("/tmp/adapter/source"),
        }
    );
}

#[cfg(any())]
#[cfg(unix)]
#[tokio::test]
async fn python_chat_once_client_runs_entrypoint_with_chat_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-chat-once");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let entrypoint = root.join("tentgent-model-runtime-daemon");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > \"$TENTGENT_HOME/cwd.txt\"\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/args.txt\"\nprintf 'answer'\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver { entrypoint };
    let client = PythonChatOnceRuntimeClient::new(&executable_resolver);
    let response = client
        .generate_chat(ChatRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request: chat_request(false),
        })
        .await
        .expect("generate chat");

    assert_eq!(response.text, "answer");
    assert_eq!(response.finish_reason, ChatFinishReason::Stop);
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
    assert!(args.contains("--adapter-ref\n"));
    assert!(args.contains(&format!("{}\n", adapter_ref())));
    assert!(args.contains("--message\n"));
    assert!(args.contains("user:hello\n"));
}

#[cfg(any())]
#[cfg(unix)]
#[tokio::test]
async fn python_chat_once_client_streams_stdout_events() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-chat-once-stream");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let entrypoint = root.join("tentgent-model-runtime-daemon");
    fs::write(&entrypoint, "#!/bin/sh\nprintf 'hello'\n").expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver { entrypoint };
    let client = PythonChatOnceRuntimeClient::new(&executable_resolver);
    let mut events = Vec::new();
    let response = client
        .stream_chat(
            ChatRuntimeRequest {
                layout: runtime_layout(&home),
                runtime: python_runtime(&project, &env),
                request: chat_request(true),
            },
            &mut |event| events.push(event),
        )
        .await
        .expect("stream chat");

    assert_eq!(response.text, "hello");
    assert_eq!(
        events.last(),
        Some(&ChatStreamEvent::Done {
            finish_reason: ChatFinishReason::Stop
        })
    );
    assert!(events
        .iter()
        .any(|event| matches!(event, ChatStreamEvent::Delta { text } if text == "hello")));
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

struct FakeAdapterCompatibility {
    adapter: AdapterInspection,
}

impl AdapterCompatibilityCheckUseCase for FakeAdapterCompatibility {
    fn check_adapter_compatibility(
        &self,
        request: AdapterCompatibilityCheckRequest,
    ) -> KernelResult<AdapterCompatibilityCheckResult> {
        let layout = runtime_layout(
            request
                .layout
                .home_dir
                .as_deref()
                .unwrap_or(Path::new("/tmp")),
        );
        Ok(AdapterCompatibilityCheckResult {
            store: AdapterStoreLayout::from_adapters_dir(layout.adapters_dir.clone()),
            adapter: self.adapter.clone(),
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
        assert_eq!(entrypoint, RuntimeEntrypoint::ModelRuntimeDaemon);
        Ok(self.entrypoint.clone())
    }
}

fn chat_request(stream: bool) -> ChatRequest {
    ChatRequest {
        target: ResolvedChatTarget {
            runtime: ChatRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: ChatBackend::TransformersPeft,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::Chat],
            },
            adapter: Some(ResolvedChatAdapter {
                adapter_ref: adapter_ref(),
                backend: AdapterBackendSupport::TransformersPeft,
                source_path: PathBuf::from("/tmp/adapter/source"),
            }),
        },
        prompt: ChatPrompt::new(vec![ChatMessage::user("hello").expect("message")])
            .expect("prompt"),
        options: ChatGenerationOptions {
            max_tokens: Some(7),
            temperature: Some(0.25),
            stream,
        },
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

fn adapter_inspection() -> AdapterInspection {
    AdapterInspection {
        metadata: AdapterMetadata {
            adapter_ref: adapter_ref(),
            short_ref: adapter_ref().short_ref().to_string(),
            adapter_format: AdapterFormat::Peft,
            adapter_type: AdapterType::Lora,
            target_capability: Some(ModelCapability::Chat),
            base_model_ref: Some(model_ref()),
            base_model_source_repo: Some("org/model".to_string()),
            base_model_source_revision: Some("main".to_string()),
            model_family: None,
            backend_support: vec![AdapterBackendSupport::TransformersPeft],
            control_kind: None,
            weight_file: None,
            trigger_words: Vec::new(),
            recommended_scale: None,
            source_kind: AdapterSourceKind::Local,
            source_repo: None,
            source_revision: None,
            source_path: Some("/tmp/adapter/source".to_string()),
            training_dataset_ref: None,
            training_run_ref: None,
            training_config_ref: None,
            file_count: 1,
            total_bytes: 10,
            imported_at: "2026-01-01T00:00:00Z".to_string(),
        },
        store_path: PathBuf::from("/tmp/adapter/store"),
        manifest_path: PathBuf::from("/tmp/adapter/manifest.json"),
        source_path: PathBuf::from("/tmp/adapter/source"),
    }
}

fn model_ref() -> ModelRef {
    ModelRef::parse("a".repeat(64)).expect("model ref")
}

fn adapter_ref() -> AdapterRef {
    AdapterRef::parse("b".repeat(64)).expect("adapter ref")
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
        python_env_dir: home.join("python"),
        bootstrap_dir: home.join("bootstrap"),
        bootstrap_uv_dir: home.join("bootstrap").join("uv"),
        bootstrap_uv_cache_dir: home.join("bootstrap").join("uv-cache"),
        capabilities_path: home.join("runtime").join("capabilities.toml"),
        auth_metadata_path: home.join("runtime").join("auth.toml"),
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
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("tentgent-{label}-{}-{nanos}", std::process::id()))
}
