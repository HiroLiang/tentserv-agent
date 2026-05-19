use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    VisionChatOutputFormat, VisionChatRequest, VisionChatResponse, VisionChatRuntimeTarget,
};
use super::super::ports::{VisionChatRuntimeClient, VisionChatRuntimeRequest, VisionPortFuture};

/// Executes prepared vision-chat requests through the Python once entrypoint.
pub struct PythonVisionChatOnceRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonVisionChatOnceRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn generate_blocking(
        &self,
        request: VisionChatRuntimeRequest,
    ) -> KernelResult<VisionChatResponse> {
        let output = self
            .command_for_request(&request)?
            .output()
            .map_err(|error| {
                vision_runtime_error(format!("failed to run vision chat runtime: {error}"))
            })?;

        if !output.status.success() {
            return Err(vision_runtime_error(format_process_failure(
                "vision chat runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        let parsed: VisionChatRuntimeOutput =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                vision_runtime_error(format!(
                    "failed to parse vision chat runtime output: {error}"
                ))
            })?;

        Ok(VisionChatResponse {
            output_format: parsed.output_format,
            media_type: parsed.media_type,
            text: parsed.text,
            finish_reason: parsed.finish_reason,
        })
    }

    fn command_for_request(&self, request: &VisionChatRuntimeRequest) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::VisionChatOnce)?;
        let model_ref = local_model_ref(&request.request);

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .arg("--model-ref")
            .arg(model_ref)
            .arg("--home")
            .arg(&request.layout.home_dir)
            .arg("--image-path")
            .arg(&request.request.image_path)
            .arg("--prompt")
            .arg(&request.request.prompt.prompt)
            .arg("--format")
            .arg(request.request.output_format.as_str())
            .env("TENTGENT_HOME", &request.layout.home_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(system_prompt) = &request.request.prompt.system_prompt {
            command.arg("--system-prompt").arg(system_prompt);
        }
        if let Some(max_tokens) = request.request.options.max_tokens {
            command.arg("--max-tokens").arg(max_tokens.to_string());
        }
        if let Some(temperature) = request.request.options.temperature {
            command.arg("--temperature").arg(temperature.to_string());
        }

        Ok(command)
    }
}

impl VisionChatRuntimeClient for PythonVisionChatOnceRuntimeClient<'_> {
    fn generate_vision_chat(
        &'_ self,
        request: VisionChatRuntimeRequest,
    ) -> VisionPortFuture<'_, VisionChatResponse> {
        Box::pin(async move { self.generate_blocking(request) })
    }
}

#[derive(Debug, Deserialize)]
struct VisionChatRuntimeOutput {
    output_format: VisionChatOutputFormat,
    media_type: String,
    text: String,
    finish_reason: String,
}

fn local_model_ref(request: &VisionChatRequest) -> &str {
    match &request.target.runtime {
        VisionChatRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
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

fn vision_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::VisionRuntimeUnavailable(message.into())
}
