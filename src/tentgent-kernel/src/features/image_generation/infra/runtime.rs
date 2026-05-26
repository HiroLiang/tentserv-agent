use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::features::runtime::infra::{ModelRuntimeCapability, ModelRuntimeDaemonSupervisor};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    ImageGenerationInput, ImageGenerationRequest, ImageGenerationResponse,
    ImageGenerationRuntimeTarget,
};
use super::super::ports::{
    ImageGenerationPortFuture, ImageGenerationRuntimeClient, ImageGenerationRuntimeRequest,
};

/// Executes prepared image-generation requests through the model-runtime HTTP daemon.
pub struct PythonImageGenerationModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonImageGenerationModelRuntimeClient<'a> {
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
        request: ImageGenerationRuntimeRequest,
    ) -> KernelResult<ImageGenerationResponse> {
        let model_ref = local_model_ref(&request.request);
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::ImageGeneration,
                model_ref,
            )
            .await?;
        if matches!(request.request.input, ImageGenerationInput::Control { .. })
            && request.request.target.control.is_none()
        {
            return Err(image_generation_runtime_error(
                "image control workflow requires a resolved control adapter",
            ));
        }
        let path = image_route_path(&request.request.input);
        let payload = image_payload(request.request);
        let response: ImageGenerationResponsePayload = self
            .supervisor
            .post_json(&endpoint, path, &payload, image_generation_runtime_error)
            .await?;
        Ok(ImageGenerationResponse {
            output_format: super::super::domain::ImageGenerationOutputFormat::from_str(
                &response.output_format,
            )
            .map_err(|err| {
                image_generation_runtime_error(format!(
                    "failed to decode image generation output format: {err}"
                ))
            })?,
            media_type: response.media_type,
            output_path: response.output_path,
            total_bytes: response.total_bytes,
            width: response.width,
            height: response.height,
            seed: response.seed,
        })
    }
}

impl ImageGenerationRuntimeClient for PythonImageGenerationModelRuntimeClient<'_> {
    fn generate_image(
        &'_ self,
        request: ImageGenerationRuntimeRequest,
    ) -> ImageGenerationPortFuture<'_, ImageGenerationResponse> {
        Box::pin(async move { self.generate_http(request).await })
    }
}

#[derive(Debug, Serialize)]
struct ImagePayload {
    prompt: String,
    output_path: String,
    output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    negative_prompt: Option<String>,
    width: u32,
    height: u32,
    steps: u32,
    guidance_scale: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_image_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_image_media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mask_image_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mask_image_media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    control_image_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    control_image_media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    control_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    control_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter: Option<ImageAdapterPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    control: Option<ImageControlAdapterPayload>,
}

#[derive(Debug, Serialize)]
struct ImageAdapterPayload {
    adapter_ref: String,
    source_path: String,
    lora_scale: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight_file: Option<String>,
}

#[derive(Debug, Serialize)]
struct ImageControlAdapterPayload {
    control_ref: String,
    source_path: String,
    control_kind: String,
}

#[derive(Debug, Deserialize)]
struct ImageGenerationResponsePayload {
    output_format: String,
    media_type: String,
    output_path: PathBuf,
    total_bytes: u64,
    width: u32,
    height: u32,
    seed: Option<u64>,
}

fn image_route_path(input: &ImageGenerationInput) -> &'static str {
    match input {
        ImageGenerationInput::TextToImage => "/v1/images/generations",
        ImageGenerationInput::ImageToImage { .. } => "/v1/images/transforms",
        ImageGenerationInput::Inpaint { .. } => "/v1/images/inpaint",
        ImageGenerationInput::Control { .. } => "/v1/images/control",
    }
}

fn image_payload(request: ImageGenerationRequest) -> ImagePayload {
    let mut payload = ImagePayload {
        prompt: request.prompt.prompt,
        output_path: request.output_path.display().to_string(),
        output_format: request.output_format.as_str().to_string(),
        negative_prompt: request.prompt.negative_prompt,
        width: request.options.dimensions.width,
        height: request.options.dimensions.height,
        steps: request.options.steps,
        guidance_scale: request.options.guidance_scale,
        seed: request.options.seed,
        input_image_path: None,
        input_image_media_type: None,
        mask_image_path: None,
        mask_image_media_type: None,
        strength: None,
        control_image_path: None,
        control_image_media_type: None,
        control_kind: None,
        control_strength: None,
        adapter: request.target.adapter.map(|adapter| ImageAdapterPayload {
            adapter_ref: adapter.adapter_ref.to_string(),
            source_path: adapter.source_path.display().to_string(),
            lora_scale: adapter.scale.as_f32(),
            weight_file: adapter.weight_file,
        }),
        control: request
            .target
            .control
            .map(|control| ImageControlAdapterPayload {
                control_ref: control.adapter_ref.to_string(),
                source_path: control.source_path.display().to_string(),
                control_kind: control.control_kind.as_str().to_string(),
            }),
    };

    match request.input {
        ImageGenerationInput::TextToImage => {}
        ImageGenerationInput::ImageToImage {
            image_path,
            media_type,
            strength,
        } => {
            payload.input_image_path = Some(image_path.display().to_string());
            payload.input_image_media_type = media_type;
            payload.strength = Some(strength.as_f32());
        }
        ImageGenerationInput::Inpaint {
            image_path,
            image_media_type,
            mask_path,
            mask_media_type,
            strength,
        } => {
            payload.input_image_path = Some(image_path.display().to_string());
            payload.input_image_media_type = image_media_type;
            payload.mask_image_path = Some(mask_path.display().to_string());
            payload.mask_image_media_type = mask_media_type;
            payload.strength = Some(strength.as_f32());
        }
        ImageGenerationInput::Control {
            control_image_path,
            control_image_media_type,
            control_kind,
            control_strength,
        } => {
            payload.control_image_path = Some(control_image_path.display().to_string());
            payload.control_image_media_type = control_image_media_type;
            payload.control_kind = Some(control_kind.as_str().to_string());
            payload.control_strength = Some(control_strength.as_f32());
        }
    }
    payload
}

fn local_model_ref(request: &ImageGenerationRequest) -> &str {
    match &request.target.runtime {
        ImageGenerationRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
    }
}

fn image_generation_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::ImageGenerationRuntimeUnavailable(message.into())
}
