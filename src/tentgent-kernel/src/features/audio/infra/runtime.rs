use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    AudioTranscriptionOutputFormat, AudioTranscriptionRequest, AudioTranscriptionResponse,
    AudioTranscriptionRuntimeTarget,
};
use super::super::ports::{
    AudioPortFuture, AudioTranscriptionRuntimeClient, AudioTranscriptionRuntimeRequest,
};

/// Executes prepared audio transcription requests through the Python batch entrypoint.
pub struct PythonAudioTranscriptionBatchRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonAudioTranscriptionBatchRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn transcribe_blocking(
        &self,
        request: AudioTranscriptionRuntimeRequest,
    ) -> KernelResult<AudioTranscriptionResponse> {
        let output = self
            .command_for_request(&request)?
            .output()
            .map_err(|error| {
                audio_runtime_error(format!(
                    "failed to run audio transcription runtime: {error}"
                ))
            })?;

        if !output.status.success() {
            return Err(audio_runtime_error(format_process_failure(
                "audio transcription runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        let parsed: AudioTranscriptionRuntimeOutput = serde_json::from_slice(&output.stdout)
            .map_err(|error| {
                audio_runtime_error(format!(
                    "failed to parse audio transcription runtime output: {error}"
                ))
            })?;

        Ok(AudioTranscriptionResponse {
            output_format: parsed.output_format,
            media_type: parsed.media_type,
            output_path: parsed.output_path,
            total_bytes: parsed.total_bytes,
            text: parsed.text,
        })
    }

    fn command_for_request(
        &self,
        request: &AudioTranscriptionRuntimeRequest,
    ) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::AudioTranscriptionBatch)?;
        let model_ref = local_model_ref(&request.request);

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .arg("--model-ref")
            .arg(model_ref)
            .arg("--home")
            .arg(&request.layout.home_dir)
            .arg("--input-path")
            .arg(&request.request.input_path)
            .arg("--output-path")
            .arg(&request.request.output_path)
            .arg("--format")
            .arg(request.request.output_format.as_str())
            .env("TENTGENT_HOME", &request.layout.home_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(language) = &request.request.language {
            command.arg("--language").arg(language);
        }
        if request.request.timestamps {
            command.arg("--timestamps");
        }

        Ok(command)
    }
}

impl AudioTranscriptionRuntimeClient for PythonAudioTranscriptionBatchRuntimeClient<'_> {
    fn transcribe_audio(
        &'_ self,
        request: AudioTranscriptionRuntimeRequest,
    ) -> AudioPortFuture<'_, AudioTranscriptionResponse> {
        Box::pin(async move { self.transcribe_blocking(request) })
    }
}

#[derive(Debug, Deserialize)]
struct AudioTranscriptionRuntimeOutput {
    output_format: AudioTranscriptionOutputFormat,
    media_type: String,
    output_path: std::path::PathBuf,
    total_bytes: u64,
    text: Option<String>,
}

fn local_model_ref(request: &AudioTranscriptionRequest) -> &str {
    match &request.target.runtime {
        AudioTranscriptionRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
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

fn audio_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::AudioRuntimeUnavailable(message.into())
}
