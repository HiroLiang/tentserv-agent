use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{RerankRequest, RerankResponse, RerankRuntimeTarget, RerankScore};
use super::super::ports::{RerankPortFuture, RerankRuntimeClient, RerankRuntimeRequest};

/// Executes prepared rerank requests through the `tentgent-rerank-once` Python entrypoint.
pub struct PythonRerankOnceRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonRerankOnceRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn rerank_blocking(&self, request: RerankRuntimeRequest) -> KernelResult<RerankResponse> {
        let output = self
            .command_for_request(&request)?
            .output()
            .map_err(|error| {
                rerank_runtime_error(format!("failed to run rerank runtime: {error}"))
            })?;

        if !output.status.success() {
            return Err(rerank_runtime_error(format_process_failure(
                "rerank runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        let parsed: RerankRuntimeOutput =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                rerank_runtime_error(format!("failed to parse rerank runtime output: {error}"))
            })?;
        Ok(RerankResponse {
            data: parsed
                .data
                .into_iter()
                .map(|item| RerankScore {
                    index: item.index,
                    score: item.score,
                })
                .collect(),
        })
    }

    fn command_for_request(&self, request: &RerankRuntimeRequest) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::RerankOnce)?;
        let model_ref = local_model_ref(&request.request);

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .arg("--model-ref")
            .arg(model_ref)
            .arg("--home")
            .arg(&request.layout.home_dir)
            .arg("--query")
            .arg(&request.request.input.query)
            .env("TENTGENT_HOME", &request.layout.home_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for document in &request.request.input.documents {
            command.arg("--document").arg(document);
        }
        if let Some(top_n) = request.request.input.top_n {
            command.arg("--top-n").arg(top_n.to_string());
        }

        Ok(command)
    }
}

impl RerankRuntimeClient for PythonRerankOnceRuntimeClient<'_> {
    fn rerank(&'_ self, request: RerankRuntimeRequest) -> RerankPortFuture<'_, RerankResponse> {
        Box::pin(async move { self.rerank_blocking(request) })
    }
}

#[derive(Debug, Deserialize)]
struct RerankRuntimeOutput {
    data: Vec<RerankRuntimeScore>,
}

#[derive(Debug, Deserialize)]
struct RerankRuntimeScore {
    index: usize,
    score: f32,
}

fn local_model_ref(request: &RerankRequest) -> &str {
    match &request.target.runtime {
        RerankRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
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

fn rerank_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::RerankRuntimeUnavailable(message.into())
}
