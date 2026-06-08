use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tentgent_kernel::features::cloud::{
    domain::{CloudEndpointCapability, CloudImageGenerationRequest},
    infra::ReqwestCloudModelClient,
};

use crate::{
    provider_compat::{ensure_provider_capability, ProviderCompatRejection},
    time::unix_timestamp_seconds,
};

use super::{error::CloudServerError, CloudServerState};

pub(super) async fn images(
    State(state): State<CloudServerState>,
    Json(request): Json<ImageRequest>,
) -> Result<Json<ImageResponse>, CloudServerError> {
    request.reject_unsupported()?;
    ensure_provider_capability(
        state.config.provider,
        CloudEndpointCapability::ImageGeneration,
    )?;
    let client = ReqwestCloudModelClient::new()?;
    let response = client
        .generate_image(
            CloudImageGenerationRequest {
                provider: state.config.provider,
                model: state.config.provider_model.clone(),
                prompt: request.prompt,
                size: request.size,
            },
            &state.secret,
        )
        .await?;
    Ok(Json(ImageResponse {
        created: unix_timestamp_seconds(),
        data: vec![ImageData {
            b64_json: response.b64_json,
        }],
    }))
}

#[derive(Debug, Deserialize)]
pub(super) struct ImageRequest {
    pub(super) prompt: String,
    pub(super) size: Option<String>,
    pub(super) response_format: Option<Value>,
    pub(super) n: Option<Value>,
}

impl ImageRequest {
    pub(super) fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.response_format.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible image generation response_format is not supported; Tentgent returns b64_json",
            ));
        }
        if self.n.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible image generation only supports one image per request today",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub(super) struct ImageResponse {
    pub(super) created: u64,
    pub(super) data: Vec<ImageData>,
}

#[derive(Debug, Serialize)]
pub(super) struct ImageData {
    pub(super) b64_json: String,
}
