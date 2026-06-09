use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tentgent_kernel::features::{
    auth::domain::Provider,
    cloud::{
        domain::{CloudChatContentPart, CloudChatMessage, CloudChatRequest},
        infra::ReqwestCloudModelClient,
    },
};

use crate::{
    provider_compat::{OpenAiChatCompatFields, OpenAiMessageCompatFields, ProviderCompatRejection},
    time::unix_timestamp_seconds,
};

use super::{error::CloudServerError, stream::stream_response, CloudServerState};

pub(super) async fn openai_chat(
    State(state): State<CloudServerState>,
    Json(request): Json<OpenAiChatRequest>,
) -> Result<Response, CloudServerError> {
    let stream = request.stream.unwrap_or(false);
    if request.has_openai_audio() && state.config.provider != Provider::OpenAI {
        return Err(ProviderCompatRejection::unsupported_capability(format!(
            "{} direct cloud chat does not support OpenAI-compatible audio requests",
            state.config.provider.display_name()
        ))
        .into());
    }
    request
        .compat
        .reject_unsupported_for_direct_cloud_openai(stream)?;
    let max_tokens = request
        .max_tokens
        .or(request.compat.max_completion_tokens());
    let response_modalities = request.compat.response_modalities();
    let audio = request.compat.audio();
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
        messages: request
            .messages
            .into_iter()
            .map(OpenAiMessage::into_cloud)
            .collect::<Result<Vec<_>, _>>()?,
        max_tokens,
        temperature: request.temperature,
        stream,
        response_modalities,
        audio,
    };
    if stream {
        return stream_response(state, cloud_request).await;
    }
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(openai_chat_response_value(
        &state.config.provider_model,
        response.text,
        response.finish_reason,
        response.audio,
    ))
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiChatRequest {
    pub(super) messages: Vec<OpenAiMessage>,
    pub(super) max_tokens: Option<u32>,
    pub(super) temperature: Option<f32>,
    pub(super) stream: Option<bool>,
    #[serde(flatten)]
    pub(super) compat: OpenAiChatCompatFields,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiMessage {
    pub(super) role: String,
    pub(super) content: OpenAiContent,
    #[serde(flatten)]
    pub(super) compat: OpenAiMessageCompatFields,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiPart>),
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiPart {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) text: Option<String>,
    pub(super) image_url: Option<OpenAiImageUrl>,
    pub(super) input_audio: Option<OpenAiInputAudio>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiImageUrl {
    pub(super) url: Option<String>,
    pub(super) detail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiInputAudio {
    pub(super) data: Option<String>,
    pub(super) format: Option<String>,
}

impl OpenAiChatRequest {
    fn has_openai_audio(&self) -> bool {
        self.compat.requests_audio_output()
            || self
                .messages
                .iter()
                .any(|message| message.content.has_openai_audio())
    }
}

impl OpenAiContent {
    fn has_openai_audio(&self) -> bool {
        match self {
            Self::Text(_) => false,
            Self::Parts(parts) => parts.iter().any(|part| part.kind == "input_audio"),
        }
    }
}

impl OpenAiMessage {
    pub(super) fn into_cloud(self) -> Result<CloudChatMessage, CloudServerError> {
        self.compat.reject_unsupported()?;
        let content = match self.content {
            OpenAiContent::Text(text) => vec![CloudChatContentPart::Text(text)],
            OpenAiContent::Parts(parts) => parts
                .into_iter()
                .map(|part| match part.kind.as_str() {
                    "text" => Ok::<CloudChatContentPart, CloudServerError>(
                        CloudChatContentPart::Text(part.text.unwrap_or_default()),
                    ),
                    "image_url" => {
                        let image_url = part
                            .image_url
                            .ok_or_else(|| openai_image_url_error("image_url is missing"))?;
                        Ok(CloudChatContentPart::ImageUrl {
                            url: openai_image_url(image_url)?,
                        })
                    }
                    "input_audio" => {
                        let input_audio = part.input_audio.ok_or_else(|| {
                            openai_input_audio_error("input_audio payload is missing")
                        })?;
                        let (data, format) = openai_input_audio(input_audio)?;
                        Ok(CloudChatContentPart::InputAudio { data, format })
                    }
                    other => Err(CloudServerError::from(
                        ProviderCompatRejection::unsupported_content(format!(
                            "unsupported OpenAI content part `{other}`"
                        )),
                    )),
                })
                .collect::<Result<Vec<_>, CloudServerError>>()?,
        };
        Ok(CloudChatMessage {
            role: self.role,
            content,
        })
    }
}

fn openai_image_url(image_url: OpenAiImageUrl) -> Result<String, CloudServerError> {
    if let Some(detail) = image_url.detail.as_deref() {
        if !matches!(detail, "auto" | "low" | "high") {
            return Err(openai_image_url_error(format!(
                "unsupported OpenAI image_url detail `{detail}`"
            )));
        }
    }
    let url = image_url
        .url
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| openai_image_url_error("image_url.url is required"))?;
    Ok(url)
}

fn openai_image_url_error(message: impl Into<String>) -> CloudServerError {
    ProviderCompatRejection::unsupported_content(message).into()
}

fn openai_input_audio(input_audio: OpenAiInputAudio) -> Result<(String, String), CloudServerError> {
    let data = input_audio
        .data
        .filter(|data| !data.trim().is_empty())
        .ok_or_else(|| openai_input_audio_error("input_audio.data is required"))?;
    let format = input_audio
        .format
        .filter(|format| matches!(format.as_str(), "wav" | "mp3"))
        .ok_or_else(|| openai_input_audio_error("input_audio.format must be wav or mp3"))?;
    Ok((data, format))
}

fn openai_input_audio_error(message: impl Into<String>) -> CloudServerError {
    ProviderCompatRejection::unsupported_content(message).into()
}

pub(super) fn openai_chat_response_value(
    model: &str,
    text: String,
    finish_reason: String,
    audio: Option<Value>,
) -> Value {
    let mut message = json!({"role": "assistant", "content": text});
    if let Some(audio) = audio {
        message["audio"] = audio;
    }
    json!({
        "id": format!("chatcmpl-{}", unix_timestamp_seconds()),
        "object": "chat.completion",
        "created": unix_timestamp_seconds(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": null
    })
}
