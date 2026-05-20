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
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeSource, RuntimeEntrypoint,
};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::features::vision::domain::{
    ResolvedVisionChatTarget, VisionChatBackend, VisionChatGenerationOptions,
    VisionChatOutputFormat, VisionChatPrompt, VisionChatRequest, VisionChatResponse,
    VisionChatRuntimeTarget,
};
use crate::features::vision::infra::{
    PythonVisionChatOnceRuntimeClient, StdVisionChatModelResolver,
};
use crate::features::vision::ports::{
    VisionChatModelResolveRequest, VisionChatModelResolver, VisionChatRuntimeClient,
    VisionChatRuntimeRequest,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

#[test]
fn vision_chat_output_format_parses_aliases_and_metadata() {
    let format = "markdown"
        .parse::<VisionChatOutputFormat>()
        .expect("markdown alias");

    assert_eq!(format, VisionChatOutputFormat::Md);
    assert_eq!(format.extension(), "md");
    assert_eq!(
        VisionChatOutputFormat::Json.media_type(),
        "application/json"
    );
    assert!("xml"
        .parse::<VisionChatOutputFormat>()
        .expect_err("unsupported")
        .to_string()
        .contains("unsupported vision chat output format"));
}

#[test]
fn vision_chat_prompt_trims_text_and_rejects_blank_prompt() {
    let prompt =
        VisionChatPrompt::new(" describe ", Some(" be terse ".to_string())).expect("prompt");

    assert_eq!(prompt.prompt, "describe");
    assert_eq!(prompt.system_prompt.as_deref(), Some("be terse"));
    assert!(VisionChatPrompt::new(" ", None).is_err());
}

#[test]
fn std_vision_chat_model_resolver_accepts_safetensors_model() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Safetensors, vec![ModelCapability::VisionChat]),
    };
    let resolver = StdVisionChatModelResolver::new(&catalog);

    let result = resolver
        .resolve_vision_chat_model(VisionChatModelResolveRequest {
            layout: layout_input(unique_path("vision-model-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve vision chat model");

    assert_eq!(result.model.metadata.model_ref, model_ref());
    assert_eq!(
        result.target,
        VisionChatRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: VisionChatBackend::TransformersImageTextToText,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::VisionChat],
        }
    );
}

#[test]
fn std_vision_chat_model_resolver_rejects_non_vision_models() {
    for capability in [
        ModelCapability::Chat,
        ModelCapability::Embedding,
        ModelCapability::Rerank,
        ModelCapability::AudioTranscription,
        ModelCapability::AudioSpeech,
        ModelCapability::ImageGeneration,
    ] {
        let catalog = FakeModelCatalog {
            metadata: model_metadata(ModelFormat::Safetensors, vec![capability]),
        };
        let resolver = StdVisionChatModelResolver::new(&catalog);

        let err = resolver
            .resolve_vision_chat_model(VisionChatModelResolveRequest {
                layout: layout_input(unique_path("vision-non-vision-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-vision model");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err
            .to_string()
            .contains("requires model capability `vision-chat`"));
    }
}

#[test]
fn std_vision_chat_model_resolver_rejects_unsupported_format() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(ModelFormat::Gguf, vec![ModelCapability::VisionChat]),
    };
    let resolver = StdVisionChatModelResolver::new(&catalog);

    let err = resolver
        .resolve_vision_chat_model(VisionChatModelResolveRequest {
            layout: layout_input(unique_path("vision-gguf-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect_err("unsupported backend");

    assert!(matches!(err, KernelError::UnsupportedTarget(_)));
    assert!(err.to_string().contains("does not support `gguf`"));
}

#[cfg(unix)]
#[tokio::test]
async fn python_vision_chat_once_client_runs_entrypoint_with_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-vision-chat");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let entrypoint = root.join("tentgent-vision-chat-once");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > \"$TENTGENT_HOME/cwd.txt\"\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/args.txt\"\nprintf '{\"output_format\":\"md\",\"media_type\":\"text/markdown\",\"text\":\"a cat\",\"finish_reason\":\"stop\"}'\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver { entrypoint };
    let client = PythonVisionChatOnceRuntimeClient::new(&executable_resolver);
    let response = client
        .generate_vision_chat(VisionChatRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request: vision_request(&home),
        })
        .await
        .expect("vision chat");

    assert_eq!(
        response,
        VisionChatResponse {
            output_format: VisionChatOutputFormat::Md,
            media_type: "text/markdown".to_string(),
            text: "a cat".to_string(),
            finish_reason: "stop".to_string(),
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
    assert!(args.contains("--image-path\n"));
    assert!(args.contains("image.png\n"));
    assert!(args.contains("--prompt\n"));
    assert!(args.contains("Describe this image.\n"));
    assert!(args.contains("--system-prompt\n"));
    assert!(args.contains("Be concise.\n"));
    assert!(args.contains("--format\nmd\n"));
    assert!(args.contains("--max-tokens\n64\n"));
    assert!(args.contains("--temperature\n0.2\n"));
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
        assert_eq!(entrypoint, RuntimeEntrypoint::VisionChatOnce);
        Ok(self.entrypoint.clone())
    }
}

fn vision_request(home: &Path) -> VisionChatRequest {
    VisionChatRequest {
        target: ResolvedVisionChatTarget {
            runtime: VisionChatRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: VisionChatBackend::TransformersImageTextToText,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::VisionChat],
            },
        },
        image_path: home.join("image.png"),
        image_media_type: Some("image/png".to_string()),
        prompt: VisionChatPrompt::new("Describe this image.", Some("Be concise.".to_string()))
            .expect("prompt"),
        output_format: VisionChatOutputFormat::Md,
        options: VisionChatGenerationOptions {
            max_tokens: Some(64),
            temperature: Some(0.2),
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

fn model_ref() -> ModelRef {
    ModelRef::parse("7".repeat(64)).expect("model ref")
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
