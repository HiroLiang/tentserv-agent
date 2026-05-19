use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    EmbeddingRequest, EmbeddingResponse, EmbeddingRuntimeTarget, EmbeddingVector,
};
use super::super::ports::{EmbeddingPortFuture, EmbeddingRuntimeClient, EmbeddingRuntimeRequest};

/// Executes prepared embedding requests through the `tentgent-embed-once` Python entrypoint.
pub struct PythonEmbeddingOnceRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonEmbeddingOnceRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn embed_blocking(&self, request: EmbeddingRuntimeRequest) -> KernelResult<EmbeddingResponse> {
        let output = self
            .command_for_request(&request)?
            .output()
            .map_err(|error| {
                embedding_runtime_error(format!("failed to run embedding runtime: {error}"))
            })?;

        if !output.status.success() {
            return Err(embedding_runtime_error(format_process_failure(
                "embedding runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        let parsed: EmbeddingRuntimeOutput =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                embedding_runtime_error(format!(
                    "failed to parse embedding runtime output: {error}"
                ))
            })?;
        Ok(EmbeddingResponse {
            data: parsed
                .data
                .into_iter()
                .map(|item| EmbeddingVector {
                    index: item.index,
                    embedding: item.embedding,
                })
                .collect(),
        })
    }

    fn command_for_request(&self, request: &EmbeddingRuntimeRequest) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::EmbeddingOnce)?;
        let model_ref = local_model_ref(&request.request);

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .arg("--model-ref")
            .arg(model_ref)
            .arg("--home")
            .arg(&request.layout.home_dir)
            .env("TENTGENT_HOME", &request.layout.home_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for item in &request.request.input.items {
            command.arg("--input").arg(item);
        }

        Ok(command)
    }
}

impl EmbeddingRuntimeClient for PythonEmbeddingOnceRuntimeClient<'_> {
    fn embed<'a>(
        &'a self,
        request: EmbeddingRuntimeRequest,
    ) -> EmbeddingPortFuture<'a, EmbeddingResponse> {
        Box::pin(async move { self.embed_blocking(request) })
    }
}

#[derive(Debug, Deserialize)]
struct EmbeddingRuntimeOutput {
    data: Vec<EmbeddingRuntimeVector>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingRuntimeVector {
    index: usize,
    embedding: Vec<f32>,
}

fn local_model_ref(request: &EmbeddingRequest) -> &str {
    match &request.target.runtime {
        EmbeddingRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
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

fn embedding_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::EmbeddingRuntimeUnavailable(message.into())
}
