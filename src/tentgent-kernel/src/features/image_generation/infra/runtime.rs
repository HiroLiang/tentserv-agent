use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    ImageGenerationOutputFormat, ImageGenerationRequest, ImageGenerationResponse,
    ImageGenerationRuntimeTarget,
};
use super::super::ports::{
    ImageGenerationPortFuture, ImageGenerationRuntimeClient, ImageGenerationRuntimeRequest,
};

/// Executes prepared image-generation requests through the Python once entrypoint.
pub struct PythonImageGenerationOnceRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonImageGenerationOnceRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn generate_blocking(
        &self,
        request: ImageGenerationRuntimeRequest,
    ) -> KernelResult<ImageGenerationResponse> {
        let output = self
            .command_for_request(&request)?
            .output()
            .map_err(|error| {
                image_generation_runtime_error(format!(
                    "failed to run image generation runtime: {error}"
                ))
            })?;

        if !output.status.success() {
            return Err(image_generation_runtime_error(format_process_failure(
                "image generation runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        let parsed: ImageGenerationRuntimeOutput =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                image_generation_runtime_error(format!(
                    "failed to parse image generation runtime output: {error}"
                ))
            })?;

        Ok(ImageGenerationResponse {
            output_format: parsed.output_format,
            media_type: parsed.media_type,
            output_path: parsed.output_path,
            total_bytes: parsed.total_bytes,
            width: parsed.width,
            height: parsed.height,
            seed: parsed.seed,
        })
    }

    fn command_for_request(
        &self,
        request: &ImageGenerationRuntimeRequest,
    ) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::ImageGenerateOnce)?;
        let model_ref = local_model_ref(&request.request);

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .arg("--model-ref")
            .arg(model_ref)
            .arg("--home")
            .arg(&request.layout.home_dir)
            .arg("--prompt")
            .arg(&request.request.prompt.prompt)
            .arg("--output-path")
            .arg(&request.request.output_path)
            .arg("--format")
            .arg(request.request.output_format.as_str())
            .arg("--width")
            .arg(request.request.options.dimensions.width.to_string())
            .arg("--height")
            .arg(request.request.options.dimensions.height.to_string())
            .arg("--steps")
            .arg(request.request.options.steps.to_string())
            .arg("--guidance-scale")
            .arg(request.request.options.guidance_scale.to_string())
            .env("TENTGENT_HOME", &request.layout.home_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(negative_prompt) = &request.request.prompt.negative_prompt {
            command.arg("--negative-prompt").arg(negative_prompt);
        }
        if let Some(seed) = request.request.options.seed {
            command.arg("--seed").arg(seed.to_string());
        }
        if let Some(adapter) = &request.request.target.adapter {
            command
                .arg("--adapter-ref")
                .arg(adapter.adapter_ref.as_str())
                .arg("--adapter-source-path")
                .arg(&adapter.source_path)
                .arg("--lora-scale")
                .arg(adapter.scale.as_f32().to_string());
            if let Some(weight_file) = &adapter.weight_file {
                command.arg("--adapter-weight-file").arg(weight_file);
            }
        }

        Ok(command)
    }
}

impl ImageGenerationRuntimeClient for PythonImageGenerationOnceRuntimeClient<'_> {
    fn generate_image(
        &'_ self,
        request: ImageGenerationRuntimeRequest,
    ) -> ImageGenerationPortFuture<'_, ImageGenerationResponse> {
        Box::pin(async move { self.generate_blocking(request) })
    }
}

#[derive(Debug, Deserialize)]
struct ImageGenerationRuntimeOutput {
    output_format: ImageGenerationOutputFormat,
    media_type: String,
    output_path: std::path::PathBuf,
    total_bytes: u64,
    width: u32,
    height: u32,
    seed: Option<u64>,
}

fn local_model_ref(request: &ImageGenerationRequest) -> &str {
    match &request.target.runtime {
        ImageGenerationRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
    }
}

fn format_process_failure(prefix: &str, code: Option<i32>, stderr: &[u8]) -> String {
    let status = code
        .map(|code| format!("with status {code}"))
        .unwrap_or_else(|| "without an exit status".to_string());
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{prefix} {status}")
    } else {
        format!("{prefix} {status}: {stderr}")
    }
}

fn image_generation_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::ImageGenerationRuntimeUnavailable(message.into())
}
