use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use serde_json::json;

use super::ModelRuntimeDaemonEndpoint;

// Python observes accepted work for 300s. Allow transport/serialization overhead;
// neither timeout cancels native loading or changes the generation's idle policy.
const PRELOAD_HTTP_TIMEOUT: Duration = Duration::from_secs(305);
static NEXT_TASK: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRuntimePreloadFailureKind {
    LoadFailed,
    /// No task was accepted; release caller claims without recording failed proof.
    NotAccepted,
    ObservationTimeout,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct ModelRuntimePreloadError {
    pub kind: ModelRuntimePreloadFailureKind,
    pub message: String,
}

impl fmt::Display for ModelRuntimePreloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ModelRuntimePreloadError {}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ModelRuntimePreloadResult {
    pub task_ref: String,
    pub model_ref: String,
    pub capability: String,
    pub process_token: String,
    pub status: String,
}

pub async fn preload_model_runtime(
    endpoint: &ModelRuntimeDaemonEndpoint,
) -> Result<ModelRuntimePreloadResult, ModelRuntimePreloadError> {
    let client = reqwest::Client::builder()
        .timeout(PRELOAD_HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(transport_error)?;
    let task_ref = format!(
        "preload-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT_TASK.fetch_add(1, Ordering::Relaxed),
    );
    preload_with_client(&client, endpoint, &task_ref).await
}

async fn preload_with_client(
    client: &reqwest::Client,
    endpoint: &ModelRuntimeDaemonEndpoint,
    task_ref: &str,
) -> Result<ModelRuntimePreloadResult, ModelRuntimePreloadError> {
    let model_ref = endpoint
        .model_ref
        .as_deref()
        .ok_or_else(|| not_accepted("managed preload requires a model-bound runtime"))?;
    if endpoint.process_token.is_empty() {
        return Err(not_accepted(
            "managed preload requires a runtime generation token",
        ));
    }
    let response = client
        .post(endpoint.url("/v1/lifecycle/preload"))
        .json(&json!({"task_ref": task_ref, "process_token": endpoint.process_token}))
        .send()
        .await
        .map_err(transport_error)?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(not_accepted(
            "Python runtime does not support managed preload; run `tentgent runtime bootstrap --profile local-model` and restart the affected runtime",
        ));
    }
    let body: serde_json::Value = response.json().await.map_err(transport_error)?;
    if !status.is_success() {
        let detail = &body["detail"];
        let code = detail["code"].as_str().unwrap_or("unknown");
        let accepted = detail["task_ref"].as_str() == Some(task_ref);
        let kind = if status == reqwest::StatusCode::GATEWAY_TIMEOUT {
            ModelRuntimePreloadFailureKind::ObservationTimeout
        } else if accepted
            && matches!(status.as_u16(), 500 | 501)
            && matches!(code, "preload_failed" | "preload_unsupported")
        {
            ModelRuntimePreloadFailureKind::LoadFailed
        } else if detail.get("task_ref").is_none()
            && matches!(
                (status.as_u16(), code),
                (400, "preload_unbound_runtime")
                    | (409, "runtime_generation_mismatch" | "runtime_closing")
                    | (422, "invalid_preload_request")
                    | (501, "preload_unsupported")
            )
        {
            ModelRuntimePreloadFailureKind::NotAccepted
        } else {
            ModelRuntimePreloadFailureKind::Unavailable
        };
        return Err(ModelRuntimePreloadError {
            kind,
            message: format!(
                "managed preload returned HTTP {status} ({code}): {}",
                detail["message"]
                    .as_str()
                    .unwrap_or("no runtime diagnostic")
            ),
        });
    }
    let result: ModelRuntimePreloadResult = serde_json::from_value(body)
        .map_err(|_| unavailable("managed preload returned an invalid completion response"))?;
    if result.status != "done"
        || result.task_ref != task_ref
        || result.model_ref != model_ref
        || result.capability != endpoint.capability.as_str()
        || result.process_token != endpoint.process_token
    {
        return Err(unavailable(
            "managed preload completion identity does not match the requested runtime/task",
        ));
    }
    Ok(result)
}

fn unavailable(message: impl Into<String>) -> ModelRuntimePreloadError {
    ModelRuntimePreloadError {
        kind: ModelRuntimePreloadFailureKind::Unavailable,
        message: message.into(),
    }
}

fn not_accepted(message: impl Into<String>) -> ModelRuntimePreloadError {
    ModelRuntimePreloadError {
        kind: ModelRuntimePreloadFailureKind::NotAccepted,
        message: message.into(),
    }
}

fn transport_error(error: reqwest::Error) -> ModelRuntimePreloadError {
    ModelRuntimePreloadError {
        kind: if error.is_timeout() { ModelRuntimePreloadFailureKind::ObservationTimeout }
            else { ModelRuntimePreloadFailureKind::Unavailable },
        message: format!("managed preload could not observe completion: {error}; accepted loading may still be running"),
    }
}

#[cfg(test)]
#[path = "preload/tests.rs"]
mod tests;
