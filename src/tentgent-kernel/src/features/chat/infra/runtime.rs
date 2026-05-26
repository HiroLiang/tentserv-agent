use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::features::adapter::domain::AdapterBackendSupport;
use crate::features::runtime::infra::{
    http_error_detail, ModelRuntimeCapability, ModelRuntimeDaemonSupervisor,
};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    ChatFinishReason, ChatRequest, ChatResponse, ChatRuntimeTarget, ChatStreamEvent,
    ResolvedChatAdapter,
};
use super::super::ports::{ChatPortFuture, ChatRuntimeClient, ChatRuntimeRequest};

/// Executes prepared chat requests through the shared model-runtime HTTP daemon.
pub struct PythonChatModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonChatModelRuntimeClient<'a> {
    pub fn new(
        executable_resolver: &'a dyn RuntimeExecutableResolver,
        supervisor: &'a ModelRuntimeDaemonSupervisor,
    ) -> Self {
        Self {
            executable_resolver,
            supervisor,
        }
    }

    async fn generate_chat_http(&self, request: ChatRuntimeRequest) -> KernelResult<ChatResponse> {
        let model_ref = local_model_ref(&request.request)?;
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::Chat,
                model_ref,
            )
            .await?;
        let payload = chat_payload(&request.request);
        let response: ChatResponsePayload = self
            .supervisor
            .post_json(&endpoint, "/v1/chat", &payload, chat_runtime_error)
            .await?;
        Ok(ChatResponse {
            text: response.text,
            finish_reason: ChatFinishReason::Stop,
        })
    }

    async fn stream_chat_http(
        &self,
        request: ChatRuntimeRequest,
        sink: &mut dyn FnMut(ChatStreamEvent),
    ) -> KernelResult<ChatResponse> {
        let model_ref = local_model_ref(&request.request)?;
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::Chat,
                model_ref,
            )
            .await?;
        let payload = chat_payload(&request.request);
        let response = self
            .supervisor
            .post_response(&endpoint, "/v1/chat/stream", &payload, chat_runtime_error)
            .await?;
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut collected = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| {
                chat_runtime_error(format!("failed to read chat stream response: {err}"))
            })?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some((event, data, consumed)) = next_sse_event(&buffer) {
                buffer.drain(..consumed);
                handle_sse_event(&event, &data, sink, &mut collected)?;
            }
        }
        if !buffer.trim().is_empty() {
            if let Some((event, data, _)) = next_sse_event(&(buffer + "\n\n")) {
                handle_sse_event(&event, &data, sink, &mut collected)?;
            }
        }

        Ok(ChatResponse {
            text: collected,
            finish_reason: ChatFinishReason::Stop,
        })
    }
}

impl ChatRuntimeClient for PythonChatModelRuntimeClient<'_> {
    fn generate_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move { self.generate_chat_http(request).await })
    }

    fn stream_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move { self.stream_chat_http(request, sink).await })
    }
}

#[derive(Debug, Serialize)]
struct ChatPayload {
    messages: Vec<ChatMessagePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter: Option<AdapterRecordPayload>,
}

#[derive(Debug, Serialize)]
struct ChatMessagePayload {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct AdapterRecordPayload {
    adapter_ref: String,
    source_path: String,
    adapter_format: String,
    adapter_type: &'static str,
    short_ref: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponsePayload {
    text: String,
}

#[derive(Debug, Deserialize)]
struct DeltaEventPayload {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ErrorEventPayload {
    message: String,
}

fn chat_payload(request: &ChatRequest) -> ChatPayload {
    ChatPayload {
        messages: request
            .prompt
            .messages
            .iter()
            .map(|message| ChatMessagePayload {
                role: message.role.as_str().to_string(),
                content: message.content.clone(),
            })
            .collect(),
        max_tokens: request.options.max_tokens,
        temperature: request.options.temperature,
        adapter: request.target.adapter.as_ref().map(adapter_payload),
    }
}

fn adapter_payload(adapter: &ResolvedChatAdapter) -> AdapterRecordPayload {
    AdapterRecordPayload {
        adapter_ref: adapter.adapter_ref.to_string(),
        source_path: adapter.source_path.display().to_string(),
        adapter_format: adapter_format_for_backend(adapter.backend).to_string(),
        adapter_type: "lora",
        short_ref: adapter.adapter_ref.short_ref().to_string(),
    }
}

fn adapter_format_for_backend(backend: AdapterBackendSupport) -> &'static str {
    match backend {
        AdapterBackendSupport::TransformersPeft => "peft",
        AdapterBackendSupport::Mlx => "mlx",
        AdapterBackendSupport::Diffusers => "diffusers-lora",
        AdapterBackendSupport::MlxDiffusion => "mlx-diffusion-lora",
        AdapterBackendSupport::LlamaCpp => "llama-cpp",
    }
}

fn local_model_ref(request: &ChatRequest) -> KernelResult<&str> {
    match &request.target.runtime {
        ChatRuntimeTarget::LocalModel { model_ref, .. } => Ok(model_ref.as_str()),
        ChatRuntimeTarget::CloudProvider { .. } => Err(KernelError::UnsupportedTarget(
            "model-runtime HTTP chat requires a local model target".to_string(),
        )),
    }
}

fn next_sse_event(buffer: &str) -> Option<(String, String, usize)> {
    let index = buffer.find("\n\n")?;
    let block = &buffer[..index];
    let mut event = None;
    let mut data = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim().to_string());
        }
    }
    Some((event.unwrap_or_default(), data.join("\n"), index + 2))
}

fn handle_sse_event(
    event: &str,
    data: &str,
    sink: &mut dyn FnMut(ChatStreamEvent),
    collected: &mut String,
) -> KernelResult<()> {
    match event {
        "started" => Ok(()),
        "delta" => {
            let payload: DeltaEventPayload = serde_json::from_str(data).map_err(|err| {
                chat_runtime_error(format!("failed to decode chat stream delta: {err}"))
            })?;
            collected.push_str(&payload.text);
            sink(ChatStreamEvent::Delta { text: payload.text });
            Ok(())
        }
        "done" => {
            sink(ChatStreamEvent::Done {
                finish_reason: ChatFinishReason::Stop,
            });
            Ok(())
        }
        "canceled" => {
            let message = "chat stream was canceled".to_string();
            sink(ChatStreamEvent::Error {
                code: "chat_runtime_canceled".to_string(),
                message: message.clone(),
            });
            Err(chat_runtime_error(message))
        }
        "error" => {
            let payload: ErrorEventPayload = serde_json::from_str(data).map_err(|err| {
                chat_runtime_error(format!("failed to decode chat stream error: {err}"))
            })?;
            sink(ChatStreamEvent::Error {
                code: "chat_runtime_failed".to_string(),
                message: payload.message.clone(),
            });
            Err(chat_runtime_error(payload.message))
        }
        "" => Ok(()),
        other => Err(chat_runtime_error(format!(
            "unsupported chat stream event `{other}`"
        ))),
    }
}

fn chat_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::ChatRuntimeUnavailable(message.into())
}

#[allow(dead_code)]
async fn decode_http_error(response: reqwest::Response) -> KernelError {
    chat_runtime_error(http_error_detail(response).await)
}
