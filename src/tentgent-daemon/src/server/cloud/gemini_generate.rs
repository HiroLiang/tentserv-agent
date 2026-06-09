use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use tentgent_kernel::features::{
    auth::domain::Provider,
    cloud::{
        domain::{CloudChatContentPart, CloudChatMessage, CloudChatRequest},
        infra::ReqwestCloudModelClient,
    },
};

use crate::provider_compat::ProviderCompatRejection;

use super::{error::CloudServerError, CloudServerState};

pub(super) async fn gemini_generate_content(
    State(state): State<CloudServerState>,
    Path(operation): Path<String>,
    Json(request): Json<GeminiGenerateContentRequest>,
) -> Result<Response, CloudServerError> {
    let cloud_request = gemini_request_into_cloud(
        request,
        &operation,
        state.config.provider,
        state.config.provider_model.clone(),
    )?;
    if cloud_request.stream {
        return gemini_stream_response(state, cloud_request).await;
    }
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(gemini_response_value(
        &state.config.provider_model,
        Some(response.text),
        Some(response.finish_reason),
    ))
    .into_response())
}

async fn gemini_stream_response(
    state: CloudServerState,
    mut request: CloudChatRequest,
) -> Result<Response, CloudServerError> {
    request.stream = false;
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(request, &state.secret).await?;
    let mut events = Vec::new();
    if !response.text.is_empty() {
        events.push(Ok(Event::default().data(
            gemini_response_value(&state.config.provider_model, Some(response.text), None)
                .to_string(),
        )));
    }
    events.push(Ok(Event::default().data(
        gemini_response_value(
            &state.config.provider_model,
            None,
            Some(response.finish_reason),
        )
        .to_string(),
    )));
    let stream = stream::iter(
        events
            .into_iter()
            .collect::<Vec<Result<Event, Infallible>>>(),
    );
    Ok(Sse::new(stream).into_response())
}

pub(super) fn gemini_response_value(
    model: &str,
    text: Option<String>,
    finish_reason: Option<String>,
) -> Value {
    let parts = text
        .map(|text| vec![json!({ "text": text })])
        .unwrap_or_default();
    json!({
        "candidates": [{
            "index": 0,
            "content": {
                "role": "model",
                "parts": parts
            },
            "finishReason": finish_reason
        }],
        "usageMetadata": null,
        "modelVersion": model,
    })
}

pub(super) fn gemini_request_into_cloud(
    request: GeminiGenerateContentRequest,
    operation: &str,
    provider: Provider,
    provider_model: String,
) -> Result<CloudChatRequest, CloudServerError> {
    request.reject_unsupported()?;
    let stream = gemini_operation_stream(operation)?;
    let mut messages = Vec::new();
    if let Some(system) = request.system_instruction {
        messages.push(CloudChatMessage {
            role: "system".to_string(),
            content: gemini_parts_into_cloud(system.parts)?,
        });
    }
    for content in request.contents {
        messages.push(CloudChatMessage {
            role: content.role.unwrap_or_else(|| "user".to_string()),
            content: gemini_parts_into_cloud(content.parts)?,
        });
    }

    Ok(CloudChatRequest {
        provider,
        model: provider_model,
        messages,
        max_tokens: request
            .generation_config
            .as_ref()
            .and_then(|config| config.max_output_tokens),
        temperature: request
            .generation_config
            .as_ref()
            .and_then(|config| config.temperature),
        stream,
        response_modalities: None,
        audio: None,
    })
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiGenerateContentRequest {
    contents: Vec<GeminiContent>,
    #[serde(alias = "systemInstruction")]
    system_instruction: Option<GeminiContent>,
    #[serde(alias = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
    tools: Option<Value>,
    #[serde(alias = "toolConfig")]
    tool_config: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiGenerationConfig {
    #[serde(alias = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiContent {
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiPart {
    text: Option<String>,
    #[serde(alias = "inlineData")]
    inline_data: Option<GeminiInlineData>,
    #[serde(alias = "fileData")]
    file_data: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiInlineData {
    #[serde(alias = "mimeType")]
    mime_type: Option<String>,
    data: Option<String>,
}

pub(super) fn gemini_parts_into_cloud(
    parts: Vec<GeminiPart>,
) -> Result<Vec<CloudChatContentPart>, CloudServerError> {
    parts
        .into_iter()
        .map(|part| {
            if let Some(text) = part.text {
                return Ok(CloudChatContentPart::Text(text));
            }
            if let Some(data) = part.inline_data {
                let media_type = gemini_image_media_type(data.mime_type)?;
                let data = gemini_image_data(data.data)?;
                return Ok(CloudChatContentPart::ImageBase64 {
                    media_type,
                    data,
                });
            }
            if part.file_data.is_some() {
                return Err(ProviderCompatRejection::unsupported_content(
                    "Gemini-compatible fileData parts are not supported by Tentgent direct cloud compatibility yet",
                )
                .into());
            }
            Err(ProviderCompatRejection::unsupported_content("unsupported Gemini part").into())
        })
        .collect()
}

fn gemini_image_media_type(media_type: Option<String>) -> Result<String, CloudServerError> {
    let media_type = media_type
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| gemini_image_error("Gemini inlineData mimeType is required"))?;
    if matches!(
        media_type.as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    ) {
        Ok(media_type)
    } else {
        Err(gemini_image_error(format!(
            "unsupported Gemini inlineData mimeType `{media_type}`"
        )))
    }
}

fn gemini_image_data(data: Option<String>) -> Result<String, CloudServerError> {
    let data = data
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| gemini_image_error("Gemini inlineData data is required"))?;
    STANDARD
        .decode(data.as_bytes())
        .map_err(|_| gemini_image_error("Gemini inlineData data must be valid base64"))?;
    Ok(data)
}

fn gemini_image_error(message: impl Into<String>) -> CloudServerError {
    ProviderCompatRejection::unsupported_content(message).into()
}

impl GeminiGenerateContentRequest {
    fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.tools.is_some() || self.tool_config.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Gemini-compatible tools require kernel tool-call support",
            ));
        }
        Ok(())
    }
}

pub(super) fn gemini_operation_stream(operation: &str) -> Result<bool, ProviderCompatRejection> {
    if operation.strip_suffix(":generateContent").is_some() {
        return Ok(false);
    }
    if operation.strip_suffix(":streamGenerateContent").is_some() {
        return Ok(true);
    }
    Err(ProviderCompatRejection::unsupported_operation(
        "unsupported Gemini generateContent operation",
    ))
}
