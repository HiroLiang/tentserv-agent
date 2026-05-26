use serde::{Deserialize, Serialize};

use crate::features::runtime::infra::{ModelRuntimeCapability, ModelRuntimeDaemonSupervisor};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    VisionChatOutputFormat, VisionChatRequest, VisionChatResponse, VisionChatRuntimeTarget,
};
use super::super::ports::{VisionChatRuntimeClient, VisionChatRuntimeRequest, VisionPortFuture};

/// Executes prepared vision-chat requests through the model-runtime HTTP daemon.
pub struct PythonVisionChatModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonVisionChatModelRuntimeClient<'a> {
    pub fn new(
        executable_resolver: &'a dyn RuntimeExecutableResolver,
        supervisor: &'a ModelRuntimeDaemonSupervisor,
    ) -> Self {
        Self {
            executable_resolver,
            supervisor,
        }
    }

    async fn generate_http(
        &self,
        request: VisionChatRuntimeRequest,
    ) -> KernelResult<VisionChatResponse> {
        let model_ref = local_model_ref(&request.request);
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::VisionChat,
                model_ref,
            )
            .await?;
        let payload = VisionChatPayload {
            image_path: request.request.image_path.display().to_string(),
            prompt: request.request.prompt.prompt,
            output_format: request.request.output_format,
            image_media_type: request.request.image_media_type,
            system_prompt: request.request.prompt.system_prompt,
            max_tokens: request.request.options.max_tokens,
            temperature: request.request.options.temperature,
        };
        let response: VisionChatResponsePayload = self
            .supervisor
            .post_json(&endpoint, "/v1/vision/chat", &payload, vision_runtime_error)
            .await?;
        Ok(VisionChatResponse {
            output_format: response.output_format,
            media_type: response.media_type,
            text: response.text,
            finish_reason: response.finish_reason,
        })
    }
}

impl VisionChatRuntimeClient for PythonVisionChatModelRuntimeClient<'_> {
    fn generate_vision_chat(
        &'_ self,
        request: VisionChatRuntimeRequest,
    ) -> VisionPortFuture<'_, VisionChatResponse> {
        Box::pin(async move { self.generate_http(request).await })
    }
}

#[derive(Debug, Serialize)]
struct VisionChatPayload {
    image_path: String,
    prompt: String,
    output_format: VisionChatOutputFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct VisionChatResponsePayload {
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

fn vision_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::VisionRuntimeUnavailable(message.into())
}
