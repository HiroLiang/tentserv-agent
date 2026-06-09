use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};
use tentgent_kernel::features::cloud::{
    domain::{CloudChatContentPart, CloudChatMessage, CloudChatRequest},
    infra::ReqwestCloudModelClient,
};

use crate::{provider_compat::ProviderCompatRejection, time::unix_timestamp_seconds};

use super::{error::CloudServerError, CloudServerState};

pub(super) async fn claude_messages(
    State(state): State<CloudServerState>,
    Json(request): Json<ClaudeMessagesRequest>,
) -> Result<Response, CloudServerError> {
    request.reject_unsupported()?;
    let mut messages = Vec::new();
    if let Some(system) = request.system {
        messages.push(CloudChatMessage::text(
            "system",
            claude_text_content(system)?,
        ));
    }
    messages.extend(
        request
            .messages
            .into_iter()
            .map(ClaudeMessage::into_cloud)
            .collect::<Result<Vec<_>, _>>()?,
    );
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
        messages,
        max_tokens: Some(request.max_tokens),
        temperature: request.temperature,
        stream: false,
        response_modalities: None,
        audio: None,
    };
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(claude_messages_response_value(
        &state.config.provider_model,
        response.text,
        response.finish_reason,
    ))
    .into_response())
}

pub(super) fn claude_messages_response_value(
    model: &str,
    text: String,
    stop_reason: String,
) -> Value {
    json!({
        "id": format!("msg-{}", unix_timestamp_seconds()),
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "model": model,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": null
    })
}

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeMessagesRequest {
    pub(super) messages: Vec<ClaudeMessage>,
    pub(super) system: Option<ClaudeContent>,
    pub(super) max_tokens: u32,
    pub(super) temperature: Option<f32>,
    pub(super) stream: Option<bool>,
    pub(super) tools: Option<Value>,
    pub(super) tool_choice: Option<Value>,
    pub(super) audio: Option<Value>,
    pub(super) modalities: Option<Value>,
    pub(super) input_audio: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeMessage {
    pub(super) role: String,
    pub(super) content: ClaudeContent,
    pub(super) audio: Option<Value>,
    pub(super) input_audio: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ClaudeContent {
    Text(String),
    Blocks(Vec<ClaudeContentBlock>),
}

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeContentBlock {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) text: Option<String>,
    pub(super) source: Option<ClaudeImageSource>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeImageSource {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) media_type: Option<String>,
    pub(super) data: Option<String>,
}

impl ClaudeMessage {
    pub(super) fn into_cloud(self) -> Result<CloudChatMessage, CloudServerError> {
        if self.audio.is_some() || self.input_audio.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible message audio fields are not supported by Tentgent direct cloud compatibility yet",
            )
            .into());
        }
        let role = claude_role(&self.role)?;
        let content = match self.content {
            ClaudeContent::Text(text) => vec![CloudChatContentPart::Text(text)],
            ClaudeContent::Blocks(blocks) => blocks
                .into_iter()
                .map(|block| match block.kind.as_str() {
                    "text" => Ok::<CloudChatContentPart, CloudServerError>(
                        CloudChatContentPart::Text(block.text.unwrap_or_default()),
                    ),
                    "image" => {
                        let source = block
                            .source
                            .ok_or_else(|| claude_image_error("Claude image source is missing"))?;
                        if source.kind != "base64" {
                            return Err(CloudServerError::from(
                                ProviderCompatRejection::unsupported_content(format!(
                                    "unsupported Claude image source `{}`",
                                    source.kind
                                )),
                            ));
                        }
                        let media_type = claude_image_media_type(source.media_type)?;
                        let data = claude_image_data(source.data)?;
                        Ok(CloudChatContentPart::ImageBase64 { media_type, data })
                    }
                    other => Err(CloudServerError::from(
                        ProviderCompatRejection::unsupported_content(format!(
                            "unsupported Claude content block `{other}`"
                        )),
                    )),
                })
                .collect::<Result<Vec<_>, CloudServerError>>()?,
        };
        Ok(CloudChatMessage { role, content })
    }
}

fn claude_image_media_type(media_type: Option<String>) -> Result<String, CloudServerError> {
    let media_type = media_type
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| claude_image_error("Claude image media_type is required"))?;
    if matches!(
        media_type.as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    ) {
        Ok(media_type)
    } else {
        Err(claude_image_error(format!(
            "unsupported Claude image media_type `{media_type}`"
        )))
    }
}

fn claude_image_data(data: Option<String>) -> Result<String, CloudServerError> {
    let data = data
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| claude_image_error("Claude image data is required"))?;
    STANDARD
        .decode(data.as_bytes())
        .map_err(|_| claude_image_error("Claude image data must be valid base64"))?;
    Ok(data)
}

fn claude_image_error(message: impl Into<String>) -> CloudServerError {
    ProviderCompatRejection::unsupported_content(message).into()
}

impl ClaudeMessagesRequest {
    pub(super) fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.tools.is_some() || self.tool_choice.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible tools require kernel tool-call support",
            ));
        }
        if self.stream.unwrap_or(false) {
            return Err(ProviderCompatRejection::unsupported_field(
                "direct cloud Claude messages do not support stream=true yet",
            ));
        }
        if self.audio.is_some() || self.modalities.is_some() || self.input_audio.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "Claude-compatible audio input and output are not supported by Tentgent direct cloud compatibility yet",
            ));
        }
        Ok(())
    }
}

fn claude_role(role: &str) -> Result<String, CloudServerError> {
    match role.trim().to_ascii_lowercase().as_str() {
        "system" => Ok("system".to_string()),
        "user" => Ok("user".to_string()),
        "assistant" => Ok("assistant".to_string()),
        "" => Err(CloudServerError::bad_request("message role is empty")),
        other => Err(CloudServerError::bad_request(format!(
            "unsupported Claude message role `{other}`"
        ))),
    }
}

pub(super) fn claude_text_content(content: ClaudeContent) -> Result<String, CloudServerError> {
    match content {
        ClaudeContent::Text(text) => Ok(text),
        ClaudeContent::Blocks(blocks) => {
            let mut text = String::new();
            for block in blocks {
                if block.kind != "text" {
                    return Err(ProviderCompatRejection::unsupported_content(format!(
                        "unsupported Claude content block `{}`",
                        block.kind
                    ))
                    .into());
                }
                text.push_str(block.text.as_deref().unwrap_or_default());
            }
            Ok(text)
        }
    }
}
