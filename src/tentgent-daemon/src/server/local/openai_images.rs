use std::path::PathBuf;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};
use tentgent_kernel::{
    features::server::domain::ServerCapability, foundation::layout::RuntimeLayout,
};

use crate::{provider_compat::ProviderCompatRejection, time::unix_timestamp_seconds};

use super::{
    capability::{ensure_local_provider_capability, ensure_model_endpoint},
    error::LocalServerError,
    evidence::record_runtime_execution_result,
    native::{NativeLocalImageGenerationRequest, NativeLocalImageGenerationResponse},
    proxy::response_from_upstream,
    LocalServerState, RUNTIME_IMAGE_GENERATIONS_PATH,
};

pub(super) async fn image_generations(
    State(state): State<LocalServerState>,
    Json(request): Json<Value>,
) -> Result<Response, LocalServerError> {
    ensure_local_provider_capability(
        state.config.capability,
        ServerCapability::ImageGeneration,
        "local image generation",
    )?;
    let endpoint = ensure_model_endpoint(&state).await?;
    let result = if local_image_generation_request_uses_openai_shape(&request) {
        let request = LocalOpenAiImageGenerationRequest::from_value(request)?;
        openai_image_generation_to_upstream(
            &state.client,
            request,
            &endpoint.base_url,
            &state.config.model_ref,
            &state.layout,
            &state.config.server_ref,
            state.config.capability,
        )
        .await
    } else {
        native_image_generation_to_upstream(&state.client, request, &endpoint.base_url).await
    };
    record_runtime_execution_result(&state, &result);
    result
}

pub(super) async fn openai_image_generation_to_upstream(
    client: &reqwest::Client,
    request: LocalOpenAiImageGenerationRequest,
    upstream_base_url: &str,
    bound_model_ref: &str,
    layout: &RuntimeLayout,
    server_ref: &str,
    capability: ServerCapability,
) -> Result<Response, LocalServerError> {
    if capability != ServerCapability::ImageGeneration {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "OpenAI-compatible local image generation requires an image-generation server; this server is bound to {}",
            capability.as_str()
        ))
        .into());
    }
    let output_path = local_openai_image_output_path(layout, server_ref);
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            RUNTIME_IMAGE_GENERATIONS_PATH
        ))
        .json(&request.into_native_image_generation_request(output_path.clone()))
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    openai_image_generation_response_from_upstream(upstream, bound_model_ref).await
}

pub(super) async fn native_image_generation_to_upstream(
    client: &reqwest::Client,
    request: Value,
    upstream_base_url: &str,
) -> Result<Response, LocalServerError> {
    let upstream = client
        .post(format!(
            "{}{}",
            upstream_base_url.trim_end_matches('/'),
            RUNTIME_IMAGE_GENERATIONS_PATH
        ))
        .json(&request)
        .send()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("model runtime proxy failed: {err}"))
        })?;
    response_from_upstream(upstream)
}

pub(super) async fn openai_image_generation_response_from_upstream(
    upstream: reqwest::Response,
    bound_model_ref: &str,
) -> Result<Response, LocalServerError> {
    if !upstream.status().is_success() {
        return response_from_upstream(upstream);
    }
    let response = upstream
        .json::<NativeLocalImageGenerationResponse>()
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!("decode image generation response failed: {err}"))
        })?;
    let bytes = tokio::fs::read(&response.output_path)
        .await
        .map_err(|err| {
            LocalServerError::bad_gateway(format!(
                "read image generation output `{}` failed: {err}",
                response.output_path
            ))
        })?;
    Ok(Json(json!({
        "created": unix_timestamp_seconds(),
        "data": [{
            "b64_json": BASE64_STANDARD.encode(bytes)
        }],
        "model": if response.model_ref.is_empty() {
            bound_model_ref.to_string()
        } else {
            response.model_ref
        }
    }))
    .into_response())
}

#[derive(Debug)]
pub(super) struct LocalOpenAiImageGenerationRequest {
    prompt: String,
    size: Option<String>,
}

impl LocalOpenAiImageGenerationRequest {
    pub(super) fn from_value(value: Value) -> Result<Self, LocalServerError> {
        let object = value.as_object().ok_or_else(|| {
            LocalServerError::bad_request("bad_request", "request body must be a JSON object")
        })?;
        let unknown = object
            .keys()
            .filter(|key| {
                !matches!(
                    key.as_str(),
                    "model" | "prompt" | "size" | "provider" | "response_format" | "n"
                )
            })
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(ProviderCompatRejection::unsupported_field(format!(
                "unsupported OpenAI-compatible image generation request fields: {}",
                unknown.join(", ")
            ))
            .into());
        }
        if object.get("model").is_some_and(|value| !value.is_string()) {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "`model` must be a string when present",
            ));
        }
        if object.get("provider").is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "local OpenAI-compatible image generation uses the bound local model and does not support provider selection",
            )
            .into());
        }
        if object.get("response_format").is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible local image generation response_format is not supported; Tentgent returns b64_json",
            )
            .into());
        }
        if object.get("n").is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible local image generation only supports one image per request today",
            )
            .into());
        }
        let prompt = object
            .get("prompt")
            .and_then(Value::as_str)
            .ok_or_else(|| LocalServerError::bad_request("bad_request", "`prompt` is required"))?
            .trim()
            .to_string();
        if prompt.is_empty() {
            return Err(LocalServerError::bad_request(
                "bad_request",
                "`prompt` must not be empty",
            ));
        }
        let size = object
            .get("size")
            .map(local_image_generation_size)
            .transpose()?;
        Ok(Self { prompt, size })
    }

    pub(super) fn into_native_image_generation_request(
        self,
        output_path: PathBuf,
    ) -> NativeLocalImageGenerationRequest {
        let dimensions = self
            .size
            .as_deref()
            .and_then(local_image_generation_dimensions);
        NativeLocalImageGenerationRequest {
            prompt: self.prompt,
            output_path: output_path.display().to_string(),
            output_format: "png".to_string(),
            width: dimensions.map(|(width, _)| width),
            height: dimensions.map(|(_, height)| height),
        }
    }
}

pub(super) fn local_image_generation_request_uses_openai_shape(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("output_path").is_none()
        || object.get("model").is_some_and(Value::is_string)
        || object.get("size").is_some()
        || object.get("provider").is_some()
        || object.get("response_format").is_some()
        || object.get("n").is_some()
}

fn local_image_generation_size(value: &Value) -> Result<String, LocalServerError> {
    let size = value.as_str().ok_or_else(|| {
        LocalServerError::bad_request("bad_request", "`size` must be a string when present")
    })?;
    if local_image_generation_dimensions(size).is_none() {
        return Err(LocalServerError::bad_request(
            "bad_request",
            "`size` must use `<width>x<height>` format",
        ));
    }
    Ok(size.to_string())
}

fn local_image_generation_dimensions(size: &str) -> Option<(u32, u32)> {
    let (width, height) = size.split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    Some((width, height))
}

pub(super) fn local_openai_image_output_path(
    layout: &tentgent_kernel::foundation::layout::RuntimeLayout,
    server_ref: &str,
) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let safe_server_ref = server_ref
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    layout
        .runtime_dir
        .join("local-openai-images")
        .join(safe_server_ref)
        .join(format!("{suffix}.png"))
}
