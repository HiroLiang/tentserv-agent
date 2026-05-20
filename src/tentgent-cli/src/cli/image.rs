use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::image_generation::{
    domain::{ImageGenerationDimensions, ImageGenerationOptions, ImageGenerationOutputFormat},
    infra::{PythonImageGenerationOnceRuntimeClient, StdImageGenerationModelResolver},
    usecases::{
        ImageGenerationPreparationRequest, ImageGenerationUseCase, StdImageGenerationUseCase,
    },
};
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::model::usecases::StdModelCatalogReadUseCase;
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::StdRuntimeResolutionUseCase;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::commands::{ImageCommands, ImageGenerateCommand};

pub async fn handle_image_command(command: ImageCommands) -> Result<()> {
    match command {
        ImageCommands::Generate(command) => handle_image_generate_command(command).await,
    }
}

async fn handle_image_generate_command(command: ImageGenerateCommand) -> Result<()> {
    let output_format = parse_output_format(&command.format)?;
    let output = ImageGenerateOutputTarget::prepare(&command.output, output_format)?;
    let request = image_generate_request(&command, output.runtime_path.clone(), output_format)?;

    let kernel = CliImageGenerationKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdImageGenerationModelResolver::new(&model_catalog);
    let runtime_client = PythonImageGenerationOnceRuntimeClient::new(&kernel.executable_resolver);
    let generator =
        StdImageGenerationUseCase::new(&runtime_resolution, &model_resolver, &runtime_client);

    let result = match generator.generate_image(request).await {
        Ok(result) => result,
        Err(error) => {
            output.cleanup_temp();
            return Err(image_generation_runtime_report(error.to_string()));
        }
    };

    output.finish(&result.response.output_path)
}

struct CliImageGenerationKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_catalog: FileModelCatalogStore,
}

impl CliImageGenerationKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_catalog: FileModelCatalogStore,
        }
    }
}

fn image_generate_request(
    command: &ImageGenerateCommand,
    output_path: PathBuf,
    output_format: ImageGenerationOutputFormat,
) -> Result<ImageGenerationPreparationRequest> {
    let model_selector = ModelRefSelector::parse(&command.model_ref)
        .map_err(|err| miette!("failed to parse model ref for image generation: {err}"))?;
    let prompt = non_empty_string(command.prompt.clone())
        .ok_or_else(|| miette!("image generation prompt must not be empty"))?;
    let dimensions = ImageGenerationDimensions::new(command.width, command.height)
        .map_err(|err| miette!("{err}"))?;
    let options = ImageGenerationOptions::new(
        dimensions,
        command.steps,
        command.guidance_scale,
        command.seed,
    )
    .map_err(|err| miette!("{err}"))?;

    Ok(ImageGenerationPreparationRequest {
        layout: runtime_layout_input(command.home.as_deref()),
        runtime: PythonRuntimeResolutionInput::default(),
        model_selector,
        prompt,
        negative_prompt: command.negative_prompt.clone().and_then(non_empty_string),
        output_path,
        output_format,
        options,
    })
}

fn parse_output_format(value: &str) -> Result<ImageGenerationOutputFormat> {
    value
        .parse::<ImageGenerationOutputFormat>()
        .map_err(|err| miette!("{err}"))
}

fn runtime_layout_input(home: Option<&Path>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: home.map(Path::to_path_buf),
        data_root_dir: None,
    }
}

#[derive(Debug)]
struct ImageGenerateOutputTarget {
    runtime_path: PathBuf,
    final_path: PathBuf,
}

impl ImageGenerateOutputTarget {
    fn prepare(output_path: &Path, output_format: ImageGenerationOutputFormat) -> Result<Self> {
        let final_path = absolutize_cli_path(output_path)?;
        ensure_output_path_available(&final_path)?;
        ensure_output_parent(&final_path)?;
        Ok(Self {
            runtime_path: unique_temp_output_near(&final_path, output_format),
            final_path,
        })
    }

    fn finish(&self, runtime_output_path: &Path) -> Result<()> {
        if self.final_path.exists() {
            self.cleanup_temp();
            return Err(miette!(
                "output file already exists: {}",
                self.final_path.display()
            ));
        }
        fs::rename(runtime_output_path, &self.final_path).map_err(|error| {
            miette!(
                "failed to write generated image `{}`: {error}",
                self.final_path.display()
            )
        })?;
        println!("image written: {}", self.final_path.display());
        Ok(())
    }

    fn cleanup_temp(&self) {
        let _ = fs::remove_file(&self.runtime_path);
    }
}

fn ensure_output_path_available(path: &Path) -> Result<()> {
    if path.is_dir() {
        return Err(miette!("output path is a directory: {}", path.display()));
    }
    if path.exists() {
        return Err(miette!("output file already exists: {}", path.display()));
    }
    Ok(())
}

fn ensure_output_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if parent.exists() {
        if !parent.is_dir() {
            return Err(miette!(
                "output parent path is not a directory: {}",
                parent.display()
            ));
        }
        return Ok(());
    }
    fs::create_dir_all(parent).map_err(|error| {
        miette!(
            "failed to create output directory `{}`: {error}",
            parent.display()
        )
    })
}

fn image_generation_runtime_report(message: String) -> miette::Report {
    let lower = message.to_ascii_lowercase();
    if lower.contains("local-model dependencies are not installed")
        || lower.contains("no module named 'diffusers'")
        || lower.contains("no module named 'accelerate'")
        || lower.contains("missing python package: diffusers")
        || lower.contains("missing python package: accelerate")
    {
        return miette!(
            "image generation failed: {message}\n\nruntime hint: Diffusers image generation requires the local-model Python runtime dependencies. Run `tentgent doctor`; in development run `uv sync --extra local-model` under python/tentgent-daemon."
        );
    }
    if lower.contains("mps") || lower.contains("dtype") {
        return miette!(
            "image generation failed: {message}\n\nruntime hint: this looks like a PyTorch device or dtype compatibility error. Try `TENTGENT_IMAGE_GENERATION_DEVICE=cpu` or `TENTGENT_IMAGE_GENERATION_TORCH_DTYPE=float32`, then rerun the same command."
        );
    }
    miette!("image generation failed: {message}")
}

fn unique_temp_output_near(
    final_path: &Path,
    output_format: ImageGenerationOutputFormat,
) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let filename = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");
    parent.join(format!(
        ".{filename}.tentgent-{}.tmp.{}",
        unique_suffix(),
        output_format.extension()
    ))
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

fn absolutize_cli_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(env::current_dir().into_diagnostic()?.join(path))
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jpeg_output_format_alias() {
        assert_eq!(
            parse_output_format("jpeg").expect("jpeg"),
            ImageGenerationOutputFormat::Jpeg
        );
    }

    #[test]
    fn output_path_must_not_already_exist() {
        let root = env::temp_dir().join(format!(
            "tentgent-image-output-exists-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("root");
        let path = root.join("image.png");
        fs::write(&path, b"existing").expect("existing output");

        let err = ImageGenerateOutputTarget::prepare(&path, ImageGenerationOutputFormat::Png)
            .expect_err("existing output");

        assert!(err.to_string().contains("output file already exists"));
        let _ = fs::remove_dir_all(root);
    }
}
