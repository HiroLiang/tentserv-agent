use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
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
    Ok(Json(json!({
        "id": format!("msg-{}", unix_timestamp_seconds()),
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": response.text}],
        "model": state.config.provider_model,
        "stop_reason": response.finish_reason,
        "stop_sequence": null,
        "usage": null
    }))
    .into_response())
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
}

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeMessage {
    pub(super) role: String,
    pub(super) content: ClaudeContent,
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
                        let source = block.source.ok_or_else(|| {
                            CloudServerError::bad_request("Claude image source is missing")
                        })?;
                        if source.kind != "base64" {
                            return Err(CloudServerError::from(
                                ProviderCompatRejection::unsupported_content(format!(
                                    "unsupported Claude image source `{}`",
                                    source.kind
                                )),
                            ));
                        }
                        Ok(CloudChatContentPart::ImageBase64 {
                            media_type: source.media_type.ok_or_else(|| {
                                CloudServerError::bad_request("Claude image media_type is missing")
                            })?,
                            data: source.data.ok_or_else(|| {
                                CloudServerError::bad_request("Claude image data is missing")
                            })?,
                        })
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
