use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::adapter::domain::{AdapterBackendSupport, AdapterRef, LoraScale};
use crate::features::image_generation::domain::{
    ImageGenerationBackend, ImageGenerationDimensions, ImageGenerationOptions,
    ImageGenerationOutputFormat, ImageGenerationPrompt, ImageGenerationRequest,
    ImageGenerationResponse, ImageGenerationRuntimeTarget, ResolvedImageGenerationAdapter,
    ResolvedImageGenerationTarget,
};
use crate::features::image_generation::infra::{
    PythonImageGenerationOnceRuntimeClient, StdImageGenerationModelResolver,
};
use crate::features::image_generation::ports::{
    ImageGenerationModelResolveRequest, ImageGenerationModelResolver, ImageGenerationRuntimeClient,
    ImageGenerationRuntimeRequest,
};
use crate::features::model::domain::{
    default_model_capability_source, MlxRuntimeFamily, ModelCapability, ModelFormat,
    ModelInspection, ModelMetadata, ModelRef, ModelRefSelector, ModelSourceKind, ModelStoreLayout,
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

#[test]
fn image_generation_output_format_parses_aliases_and_metadata() {
    let format = "jpeg"
        .parse::<ImageGenerationOutputFormat>()
        .expect("jpeg alias");

    assert_eq!(format, ImageGenerationOutputFormat::Jpeg);
    assert_eq!(format.extension(), "jpg");
    assert_eq!(ImageGenerationOutputFormat::Png.media_type(), "image/png");
    assert!("webp"
        .parse::<ImageGenerationOutputFormat>()
        .expect_err("unsupported")
        .to_string()
        .contains("unsupported image generation output format"));
}

#[test]
fn image_generation_prompt_trims_text_and_rejects_blank_prompt() {
    let prompt =
        ImageGenerationPrompt::new(" a neon city ", Some(" blurry ".to_string())).expect("prompt");

    assert_eq!(prompt.prompt, "a neon city");
    assert_eq!(prompt.negative_prompt.as_deref(), Some("blurry"));
    assert!(ImageGenerationPrompt::new(" ", None).is_err());
    assert!(ImageGenerationPrompt::new(
        "x".repeat(ImageGenerationPrompt::MAX_PROMPT_BYTES + 1),
        None
    )
    .is_err());
}

#[test]
fn image_generation_options_validate_model_friendly_bounds() {
    let dimensions = ImageGenerationDimensions::new(512, 768).expect("dimensions");
    let options = ImageGenerationOptions::new(dimensions, 25, 6.5, Some(42)).expect("options");

    assert_eq!(options.dimensions.width, 512);
    assert_eq!(options.dimensions.height, 768);
    assert_eq!(options.steps, 25);
    assert_eq!(options.guidance_scale, 6.5);
    assert_eq!(options.seed, Some(42));
    assert!(ImageGenerationDimensions::new(513, 512).is_err());
    assert!(ImageGenerationOptions::new(dimensions, 0, 7.5, None).is_err());
    assert!(ImageGenerationOptions::new(dimensions, 20, f32::NAN, None).is_err());
}

#[test]
fn std_image_generation_model_resolver_accepts_diffusers_model() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(
            ModelFormat::Diffusers,
            vec![ModelCapability::ImageGeneration],
        ),
    };
    let resolver = StdImageGenerationModelResolver::new(&catalog);

    let result = resolver
        .resolve_image_generation_model(ImageGenerationModelResolveRequest {
            layout: layout_input(unique_path("image-generation-model-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve image generation model");

    assert_eq!(result.model.metadata.model_ref, model_ref());
    assert_eq!(
        result.target,
        ImageGenerationRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: ImageGenerationBackend::DiffusersTextToImage,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::ImageGeneration],
        }
    );
}

#[test]
fn image_generation_backend_maps_mlx_diffusion_family_only() {
    assert_eq!(
        ImageGenerationBackend::from_model_format_and_mlx_family(ModelFormat::Diffusers, None),
        Some(ImageGenerationBackend::DiffusersTextToImage)
    );
    assert_eq!(
        ImageGenerationBackend::from_model_format_and_mlx_family(
            ModelFormat::Mlx,
            Some(MlxRuntimeFamily::Diffusion)
        ),
        Some(ImageGenerationBackend::MlxDiffusionTextToImage)
    );
    assert_eq!(
        ImageGenerationBackend::from_model_format_and_mlx_family(
            ModelFormat::Mlx,
            Some(MlxRuntimeFamily::Vlm)
        ),
        None
    );
    assert_eq!(
        ImageGenerationBackend::from_model_format_and_mlx_family(ModelFormat::Mlx, None),
        None
    );
}

#[test]
fn std_image_generation_model_resolver_accepts_mlx_diffusion_model() {
    let catalog = FakeModelCatalog {
        metadata: mlx_model_metadata(MlxRuntimeFamily::Diffusion),
    };
    let resolver = StdImageGenerationModelResolver::new(&catalog);

    let result = resolver
        .resolve_image_generation_model(ImageGenerationModelResolveRequest {
            layout: layout_input(unique_path("image-generation-mlx-diffusion-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect("resolve mlx image generation model");

    assert_eq!(
        result.target,
        ImageGenerationRuntimeTarget::LocalModel {
            model_ref: model_ref(),
            backend: ImageGenerationBackend::MlxDiffusionTextToImage,
            source_repo: Some("mlx-community/Flux-1.lite-8B-MLX-Q4".to_string()),
            source_revision: Some("main".to_string()),
            model_capabilities: vec![ModelCapability::ImageGeneration],
        }
    );
}

#[test]
fn std_image_generation_model_resolver_rejects_non_diffusion_mlx_families() {
    for family in [
        MlxRuntimeFamily::Lm,
        MlxRuntimeFamily::Vlm,
        MlxRuntimeFamily::Audio,
    ] {
        let catalog = FakeModelCatalog {
            metadata: mlx_model_metadata(family),
        };
        let resolver = StdImageGenerationModelResolver::new(&catalog);

        let err = resolver
            .resolve_image_generation_model(ImageGenerationModelResolveRequest {
                layout: layout_input(unique_path("image-generation-non-diffusion-mlx-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-diffusion mlx family");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err.to_string().contains("MLX runtime family"));
    }
}

#[test]
fn std_image_generation_model_resolver_rejects_non_image_models() {
    for capability in [
        ModelCapability::Chat,
        ModelCapability::Embedding,
        ModelCapability::Rerank,
        ModelCapability::AudioTranscription,
        ModelCapability::AudioSpeech,
        ModelCapability::VisionChat,
    ] {
        let catalog = FakeModelCatalog {
            metadata: model_metadata(ModelFormat::Diffusers, vec![capability]),
        };
        let resolver = StdImageGenerationModelResolver::new(&catalog);

        let err = resolver
            .resolve_image_generation_model(ImageGenerationModelResolveRequest {
                layout: layout_input(unique_path("image-generation-non-image-home")),
                selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
            })
            .expect_err("non-image model");

        assert!(matches!(err, KernelError::UnsupportedTarget(_)));
        assert!(err
            .to_string()
            .contains("requires model capability `image-generation`"));
    }
}

#[test]
fn std_image_generation_model_resolver_rejects_unsupported_format() {
    let catalog = FakeModelCatalog {
        metadata: model_metadata(
            ModelFormat::Safetensors,
            vec![ModelCapability::ImageGeneration],
        ),
    };
    let resolver = StdImageGenerationModelResolver::new(&catalog);

    let err = resolver
        .resolve_image_generation_model(ImageGenerationModelResolveRequest {
            layout: layout_input(unique_path("image-generation-safetensors-home")),
            selector: ModelRefSelector::parse(model_ref().short_ref()).expect("selector"),
        })
        .expect_err("unsupported backend");

    assert!(matches!(err, KernelError::UnsupportedTarget(_)));
    assert!(err.to_string().contains("does not support `safetensors`"));
}

#[cfg(unix)]
#[tokio::test]
async fn python_image_generation_once_client_runs_entrypoint_with_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-image-generation");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let output_path = home.join("image.png");
    let entrypoint = root.join("tentgent-image-generate-once");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" > \"$TENTGENT_HOME/cwd.txt\"\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/args.txt\"\nprintf '{\"output_format\":\"png\",\"media_type\":\"image/png\",\"output_path\":\"",
    )
    .expect("script prefix");
    let mut script = fs::read_to_string(&entrypoint).expect("script read");
    script.push_str(&output_path.display().to_string());
    script.push_str("\",\"total_bytes\":12,\"width\":512,\"height\":768,\"seed\":42}'\n");
    fs::write(&entrypoint, script).expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver { entrypoint };
    let client = PythonImageGenerationOnceRuntimeClient::new(&executable_resolver);
    let response = client
        .generate_image(ImageGenerationRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request: image_generation_request(&output_path),
        })
        .await
        .expect("image generation");

    assert_eq!(
        response,
        ImageGenerationResponse {
            output_format: ImageGenerationOutputFormat::Png,
            media_type: "image/png".to_string(),
            output_path: output_path.clone(),
            total_bytes: 12,
            width: 512,
            height: 768,
            seed: Some(42),
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
    assert!(args.contains("--prompt\n"));
    assert!(args.contains("A neon city.\n"));
    assert!(args.contains("--negative-prompt\n"));
    assert!(args.contains("blurry\n"));
    assert!(args.contains("--output-path\n"));
    assert!(args.contains("image.png\n"));
    assert!(args.contains("--format\npng\n"));
    assert!(args.contains("--width\n512\n"));
    assert!(args.contains("--height\n768\n"));
    assert!(args.contains("--steps\n25\n"));
    assert!(args.contains("--guidance-scale\n6.5\n"));
    assert!(args.contains("--seed\n42\n"));
}

#[cfg(unix)]
#[tokio::test]
async fn python_image_generation_once_client_passes_adapter_arguments() {
    use std::os::unix::fs::PermissionsExt;

    let root = unique_path("python-image-generation-adapter");
    let home = root.join("home");
    let project = root.join("project");
    let env = root.join("env");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&env).expect("env");
    let output_path = home.join("image.png");
    let entrypoint = root.join("tentgent-image-generate-once");
    fs::write(
        &entrypoint,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TENTGENT_HOME/args.txt\"\nprintf '{\"output_format\":\"png\",\"media_type\":\"image/png\",\"output_path\":\"",
    )
    .expect("script prefix");
    let mut script = fs::read_to_string(&entrypoint).expect("script read");
    script.push_str(&output_path.display().to_string());
    script.push_str("\",\"total_bytes\":12,\"width\":512,\"height\":768,\"seed\":42}'\n");
    fs::write(&entrypoint, script).expect("script");
    let mut permissions = fs::metadata(&entrypoint).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&entrypoint, permissions).expect("chmod");

    let executable_resolver = FakeExecutableResolver { entrypoint };
    let client = PythonImageGenerationOnceRuntimeClient::new(&executable_resolver);
    let mut request = image_generation_request(&output_path);
    request.target.adapter = Some(ResolvedImageGenerationAdapter {
        adapter_ref: adapter_ref(),
        backend: AdapterBackendSupport::Diffusers,
        source_path: home.join("adapters/store/source"),
        weight_file: Some("style.safetensors".to_string()),
        scale: LoraScale::new(0.8).expect("scale"),
    });

    client
        .generate_image(ImageGenerationRuntimeRequest {
            layout: runtime_layout(&home),
            runtime: python_runtime(&project, &env),
            request,
        })
        .await
        .expect("image generation");

    let args = fs::read_to_string(home.join("args.txt")).expect("args");
    assert!(args.contains("--adapter-ref\n"));
    assert!(args.contains(&format!("{}\n", adapter_ref())));
    assert!(args.contains("--adapter-source-path\n"));
    assert!(args.contains("adapters/store/source\n"));
    assert!(args.contains("--adapter-weight-file\n"));
    assert!(args.contains("style.safetensors\n"));
    assert!(args.contains("--lora-scale\n"));
    assert!(args.contains("0.8\n"));
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
        assert_eq!(entrypoint, RuntimeEntrypoint::ImageGenerateOnce);
        Ok(self.entrypoint.clone())
    }
}

fn image_generation_request(output_path: &Path) -> ImageGenerationRequest {
    ImageGenerationRequest {
        target: ResolvedImageGenerationTarget {
            runtime: ImageGenerationRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: ImageGenerationBackend::DiffusersTextToImage,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::ImageGeneration],
            },
            adapter: None,
        },
        prompt: ImageGenerationPrompt::new("A neon city.", Some("blurry".to_string()))
            .expect("prompt"),
        output_path: output_path.to_path_buf(),
        output_format: ImageGenerationOutputFormat::Png,
        options: ImageGenerationOptions::new(
            ImageGenerationDimensions::new(512, 768).expect("dimensions"),
            25,
            6.5,
            Some(42),
        )
        .expect("options"),
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

fn mlx_model_metadata(family: MlxRuntimeFamily) -> ModelMetadata {
    ModelMetadata {
        model_ref: model_ref(),
        short_ref: model_ref().short_ref().to_string(),
        source_kind: ModelSourceKind::HuggingFace,
        source_repo: Some("mlx-community/Flux-1.lite-8B-MLX-Q4".to_string()),
        source_revision: Some("main".to_string()),
        source_path: None,
        primary_format: ModelFormat::Mlx,
        detected_formats: vec![ModelFormat::Mlx],
        mlx_runtime_family: Some(family),
        model_capabilities: vec![ModelCapability::ImageGeneration],
        model_capability_source: default_model_capability_source(),
        file_count: 1,
        total_bytes: 10,
        imported_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn model_ref() -> ModelRef {
    ModelRef::parse("8".repeat(64)).expect("model ref")
}

fn adapter_ref() -> AdapterRef {
    AdapterRef::parse("9".repeat(64)).expect("adapter ref")
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
