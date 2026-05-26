use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use tentgent_kernel::features::{
    auth::{
        domain::{AuthEnvLoadPolicy, Provider},
        usecases::{AuthSecretResolutionRequest, AuthSecretResolverUseCase},
    },
    cloud::{domain::CloudImageGenerationRequest, infra::ReqwestCloudModelClient},
};

use crate::transport::rest::{error::RestError, state::RestState};

pub async fn generate(
    State(state): State<RestState>,
    Json(request): Json<CloudImageGenerationBody>,
) -> Result<Json<CloudImageGenerationResponseBody>, RestError> {
    let provider = request.provider.unwrap_or(Provider::OpenAI);
    let secret = state
        .app()
        .services()
        .kernel()
        .auth()
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            provider,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        ))
        .map_err(|error| RestError::kernel("image_generation_auth_failed", error))?
        .secret
        .ok_or_else(|| {
            RestError::bad_request(
                "image_generation_auth_failed",
                format!("{} API key is required", provider.display_name()),
            )
        })?;
    let client = ReqwestCloudModelClient::new()
        .map_err(|error| RestError::kernel("image_generation_runtime_failed", error))?;
    let response = client
        .generate_image(
            CloudImageGenerationRequest {
                provider,
                model: request.model.clone(),
                prompt: request.prompt,
                size: request.size,
            },
            secret.secret(),
        )
        .await
        .map_err(|error| RestError::kernel("image_generation_runtime_failed", error))?;

    Ok(Json(CloudImageGenerationResponseBody {
        created: unix_timestamp_seconds(),
        data: vec![CloudImageGenerationData {
            b64_json: response.b64_json,
        }],
    }))
}

#[derive(Debug, Deserialize)]
pub struct CloudImageGenerationBody {
    model: String,
    prompt: String,
    size: Option<String>,
    #[serde(default, deserialize_with = "deserialize_provider")]
    provider: Option<Provider>,
}

#[derive(Debug, Serialize)]
pub struct CloudImageGenerationResponseBody {
    created: u64,
    data: Vec<CloudImageGenerationData>,
}

#[derive(Debug, Serialize)]
pub struct CloudImageGenerationData {
    b64_json: String,
}

fn deserialize_provider<'de, D>(deserializer: D) -> Result<Option<Provider>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(Provider::OpenAI),
            "gemini" | "google" => Ok(Provider::Gemini),
            "anthropic" | "claude" => Ok(Provider::Anthropic),
            other => Err(serde::de::Error::custom(format!(
                "unsupported image provider `{other}`"
            ))),
        })
        .transpose()
}

fn unix_timestamp_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
