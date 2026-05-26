use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use miette::{miette, IntoDiagnostic, Result};
use serde_json::json;
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::model::usecases::StdModelCatalogReadUseCase;
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    ModelRuntimeDaemonSupervisor, StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::StdRuntimeResolutionUseCase;
use tentgent_kernel::features::vision::{
    domain::{VisionChatGenerationOptions, VisionChatOutputFormat, VisionChatResponse},
    infra::{PythonVisionChatModelRuntimeClient, StdVisionChatModelResolver},
    usecases::{StdVisionChatUseCase, VisionChatPreparationRequest, VisionChatUseCase},
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::commands::{VisionChatCommand, VisionCommands};

pub async fn handle_vision_command(command: VisionCommands) -> Result<()> {
    match command {
        VisionCommands::Chat(command) => handle_vision_chat_command(command).await,
    }
}

async fn handle_vision_chat_command(command: VisionChatCommand) -> Result<()> {
    let request = vision_chat_request(&command)?;
    let output = VisionChatOutputTarget::prepare(command.output.as_deref())?;

    let kernel = CliVisionChatKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdVisionChatModelResolver::new(&model_catalog);
    let runtime_client = PythonVisionChatModelRuntimeClient::new(
        &kernel.executable_resolver,
        &kernel.model_runtime_supervisor,
    );
    let vision = StdVisionChatUseCase::new(&runtime_resolution, &model_resolver, &runtime_client);

    let result = vision
        .generate_vision_chat(request)
        .await
        .map_err(|err| miette!("vision chat failed: {err}"))?;

    output.finish(
        result.prepared.model.metadata.model_ref.to_string(),
        &result.response,
    )
}

struct CliVisionChatKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_runtime_supervisor: ModelRuntimeDaemonSupervisor,
    model_catalog: FileModelCatalogStore,
}

impl CliVisionChatKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_runtime_supervisor: ModelRuntimeDaemonSupervisor::new(),
            model_catalog: FileModelCatalogStore,
        }
    }
}

fn vision_chat_request(command: &VisionChatCommand) -> Result<VisionChatPreparationRequest> {
    let output_format = parse_output_format(&command.format)?;
    let image_path = canonical_image_input_path(&command.image_path)?;
    let model_selector = ModelRefSelector::parse(&command.model_ref)
        .map_err(|err| miette!("failed to parse model ref for vision chat: {err}"))?;
    let prompt = non_empty_string(command.prompt.clone())
        .ok_or_else(|| miette!("vision chat prompt must not be empty"))?;

    Ok(VisionChatPreparationRequest {
        layout: runtime_layout_input(command.home.as_deref()),
        runtime: PythonRuntimeResolutionInput::default(),
        model_selector,
        image_media_type: Some(image_media_type(&image_path).to_string()),
        image_path,
        prompt,
        system_prompt: command.system_prompt.clone().and_then(non_empty_string),
        output_format,
        options: VisionChatGenerationOptions {
            max_tokens: command.max_tokens,
            temperature: command.temperature,
        },
    })
}

fn parse_output_format(value: &str) -> Result<VisionChatOutputFormat> {
    value
        .parse::<VisionChatOutputFormat>()
        .map_err(|err| miette!("{err}"))
}

fn canonical_image_input_path(path: &Path) -> Result<PathBuf> {
    let absolute = absolutize_cli_path(path)?;
    let canonical = fs::canonicalize(&absolute).map_err(|error| {
        miette!(
            "vision image path `{}` is not readable: {error}",
            absolute.display()
        )
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        miette!(
            "vision image path `{}` is not readable: {error}",
            canonical.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(miette!(
            "vision image path `{}` is not a file",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn runtime_layout_input(home: Option<&Path>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: home.map(Path::to_path_buf),
        data_root_dir: None,
    }
}

#[derive(Debug)]
struct VisionChatOutputTarget {
    final_path: Option<PathBuf>,
}

impl VisionChatOutputTarget {
    fn prepare(output_path: Option<&Path>) -> Result<Self> {
        match output_path {
            Some(path) => {
                let final_path = absolutize_cli_path(path)?;
                ensure_output_path_available(&final_path)?;
                ensure_output_parent(&final_path)?;
                Ok(Self {
                    final_path: Some(final_path),
                })
            }
            None => Ok(Self { final_path: None }),
        }
    }

    fn finish(&self, model_ref: String, response: &VisionChatResponse) -> Result<()> {
        let body = rendered_body(model_ref, response)?;
        if let Some(final_path) = &self.final_path {
            if final_path.exists() {
                return Err(miette!(
                    "output file already exists: {}",
                    final_path.display()
                ));
            }
            fs::write(final_path, &body).map_err(|error| {
                miette!(
                    "failed to write vision chat output `{}`: {error}",
                    final_path.display()
                )
            })?;
            println!("vision chat written: {}", final_path.display());
            return Ok(());
        }

        let mut stdout = io::stdout().lock();
        stdout.write_all(&body).into_diagnostic()?;
        if !body.ends_with(b"\n") {
            writeln!(stdout).into_diagnostic()?;
        }
        Ok(())
    }
}

fn rendered_body(model_ref: String, response: &VisionChatResponse) -> Result<Vec<u8>> {
    if response.output_format == VisionChatOutputFormat::Json {
        let value = json!({
            "model_ref": model_ref,
            "output_format": response.output_format.as_str(),
            "text": response.text,
            "finish_reason": response.finish_reason,
        });
        let mut body = serde_json::to_vec_pretty(&value).into_diagnostic()?;
        body.push(b'\n');
        return Ok(body);
    }

    let mut body = response.text.as_bytes().to_vec();
    if !body.ends_with(b"\n") {
        body.push(b'\n');
    }
    Ok(body)
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

fn image_media_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
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
    fn parses_markdown_output_format_alias() {
        assert_eq!(
            parse_output_format("markdown").expect("markdown"),
            VisionChatOutputFormat::Md
        );
    }

    #[test]
    fn output_path_must_not_already_exist() {
        let root = env::temp_dir().join(format!(
            "tentgent-vision-output-exists-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("root");
        let path = root.join("answer.txt");
        fs::write(&path, b"existing").expect("existing output");

        let err = VisionChatOutputTarget::prepare(Some(&path)).expect_err("existing output");

        assert!(err.to_string().contains("output file already exists"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn json_output_renders_envelope() {
        let response = VisionChatResponse {
            output_format: VisionChatOutputFormat::Json,
            media_type: "application/json".to_string(),
            text: "hello".to_string(),
            finish_reason: "stop".to_string(),
        };

        let body = rendered_body("abc".to_string(), &response).expect("body");
        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("json");

        assert_eq!(parsed["model_ref"], "abc");
        assert_eq!(parsed["text"], "hello");
    }
}
