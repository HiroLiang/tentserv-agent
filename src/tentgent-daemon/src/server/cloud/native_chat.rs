use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tentgent_kernel::features::cloud::{
    domain::{CloudChatMessage, CloudChatRequest},
    infra::ReqwestCloudModelClient,
};

use super::{error::CloudServerError, stream::stream_response, CloudServerState};

pub(super) async fn chat(
    State(state): State<CloudServerState>,
    Json(request): Json<NativeChatRequest>,
) -> Result<Response, CloudServerError> {
    let stream = request.stream.unwrap_or(false);
    let cloud_request = CloudChatRequest {
        provider: state.config.provider,
        model: state.config.provider_model.clone(),
        messages: request
            .messages
            .into_iter()
            .map(|message| CloudChatMessage::text(message.role, message.content))
            .collect(),
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        stream,
    };
    if stream {
        return stream_response(state, cloud_request).await;
    }
    let client = ReqwestCloudModelClient::new()?;
    let response = client.complete_chat(cloud_request, &state.secret).await?;
    Ok(Json(json!({
        "text": response.text,
        "finish_reason": response.finish_reason,
        "model_ref": state.config.provider_model,
        "adapter_ref": null
    }))
    .into_response())
}

#[derive(Debug, Deserialize)]
pub(super) struct NativeChatRequest {
    messages: Vec<NativeMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct NativeMessage {
    role: String,
    content: String,
}
