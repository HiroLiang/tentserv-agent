use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    VideoUnderstandingOutputFormat, VideoUnderstandingRequest, VideoUnderstandingResponse,
    VideoUnderstandingRuntimeTarget,
};
use super::super::ports::{
    VideoUnderstandingPortFuture, VideoUnderstandingRuntimeClient, VideoUnderstandingRuntimeRequest,
};

pub struct PythonVideoUnderstandingOnceRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonVideoUnderstandingOnceRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn understand_blocking(
        &self,
        request: VideoUnderstandingRuntimeRequest,
    ) -> KernelResult<VideoUnderstandingResponse> {
        let output = self
            .command_for_request(&request)?
            .output()
            .map_err(|error| {
                video_runtime_error(format!(
                    "failed to run video understanding runtime: {error}"
                ))
            })?;

        if !output.status.success() {
            return Err(video_runtime_error(format_process_failure(
                "video understanding runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        let parsed: VideoUnderstandingRuntimeOutput = serde_json::from_slice(&output.stdout)
            .map_err(|error| {
                video_runtime_error(format!(
                    "failed to parse video understanding runtime output: {error}"
                ))
            })?;

        Ok(VideoUnderstandingResponse {
            output_format: parsed.output_format,
            media_type: parsed.media_type,
            text: parsed.text,
            finish_reason: parsed.finish_reason,
            sampled_frames: parsed.sampled_frames,
        })
    }

    fn command_for_request(
        &self,
        request: &VideoUnderstandingRuntimeRequest,
    ) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::VideoUnderstandingOnce)?;
        let model_ref = local_model_ref(&request.request);

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .arg("--model-ref")
            .arg(model_ref)
            .arg("--home")
            .arg(&request.layout.home_dir)
            .arg("--video-path")
            .arg(&request.request.video_path)
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
        if let Some(sample_fps) = request.request.sampling.sample_fps {
            command.arg("--sample-fps").arg(sample_fps.to_string());
        }
        if let Some(max_frames) = request.request.sampling.max_frames {
            command.arg("--max-frames").arg(max_frames.to_string());
        }
        if let Some(max_frame_edge) = request.request.sampling.max_frame_edge {
            command
                .arg("--max-frame-edge")
                .arg(max_frame_edge.to_string());
        }
        if let Some(clip_start_seconds) = request.request.sampling.clip_start_seconds {
            command
                .arg("--clip-start-seconds")
                .arg(clip_start_seconds.to_string());
        }
        if let Some(clip_duration_seconds) = request.request.sampling.clip_duration_seconds {
            command
                .arg("--clip-duration-seconds")
                .arg(clip_duration_seconds.to_string());
        }

        Ok(command)
    }
}

impl VideoUnderstandingRuntimeClient for PythonVideoUnderstandingOnceRuntimeClient<'_> {
    fn understand_video(
        &'_ self,
        request: VideoUnderstandingRuntimeRequest,
    ) -> VideoUnderstandingPortFuture<'_, VideoUnderstandingResponse> {
        Box::pin(async move { self.understand_blocking(request) })
    }
}

#[derive(Debug, Deserialize)]
struct VideoUnderstandingRuntimeOutput {
    output_format: VideoUnderstandingOutputFormat,
    media_type: String,
    text: String,
    finish_reason: String,
    sampled_frames: Option<u32>,
}

fn local_model_ref(request: &VideoUnderstandingRequest) -> &str {
    match &request.target.runtime {
        VideoUnderstandingRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
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

fn video_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::VideoRuntimeUnavailable(message.into())
}
