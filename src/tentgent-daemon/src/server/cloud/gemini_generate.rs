use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tentgent_kernel::features::cloud::{
    domain::{CloudChatContentPart, CloudChatMessage, CloudChatRequest},
    infra::ReqwestCloudModelClient,
};

use crate::provider_compat::ProviderCompatRejection;

use super::{error::CloudServerError, stream::stream_response, CloudServerState};

pub(super) async fn gemini_generate_content(
    State(state): State<CloudServerState>,
    Path(operation): Path<String>,
    Json(request): Json<GeminiGenerateContentRequest>,
) -> Result<Response, CloudServerError> {
    request.reject_unsupported()?;
    let stream = gemini_operation_stream(&operation)?;
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
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
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
    };
    if stream {
        return stream_response(state, cloud_request).await;
    }
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(json!({
        "candidates": [{
            "index": 0,
            "content": {
                "role": "model",
                "parts": [{"text": response.text}]
            },
            "finishReason": response.finish_reason
        }],
        "usageMetadata": null,
        "modelVersion": state.config.provider_model,
    }))
    .into_response())
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
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiInlineData {
    #[serde(alias = "mimeType")]
    mime_type: String,
    data: String,
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
                return Ok(CloudChatContentPart::ImageBase64 {
                    media_type: data.mime_type,
                    data: data.data,
                });
            }
            Err(ProviderCompatRejection::unsupported_content("unsupported Gemini part").into())
        })
        .collect()
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
