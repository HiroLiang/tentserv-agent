use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tentgent_kernel::features::{
    auth::{
        domain::{AuthEnvLoadPolicy, Provider},
        usecases::{AuthSecretResolutionRequest, AuthSecretResolverUseCase},
    },
    cloud::{
        domain::{CloudEndpointCapability, CloudImageGenerationRequest},
        infra::ReqwestCloudModelClient,
    },
};

use crate::{
    provider_compat::{
        ensure_provider_capability, map_provider_kernel_error, ProviderCompatRejection,
    },
    time::unix_timestamp_seconds,
    transport::rest::{error::RestError, state::RestState},
};

pub async fn generate(
    State(state): State<RestState>,
    Json(request): Json<CloudImageGenerationBody>,
) -> Result<Json<CloudImageGenerationResponseBody>, RestError> {
    request.reject_unsupported()?;
    let provider = request.provider.unwrap_or(Provider::OpenAI);
    ensure_provider_capability(provider, CloudEndpointCapability::ImageGeneration)?;
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
        .map_err(|error| map_provider_kernel_error("image_generation_runtime_failed", error))?;

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
    response_format: Option<Value>,
    n: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_provider")]
    provider: Option<Provider>,
}

impl CloudImageGenerationBody {
    fn reject_unsupported(&self) -> Result<(), RestError> {
        if self.response_format.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible image generation response_format is not supported; Tentgent returns b64_json",
            )
            .into());
        }
        if self.n.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible image generation only supports one image per request today",
            )
            .into());
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_image_generation_body_accepts_prompt_size_and_default_provider() {
        let request: CloudImageGenerationBody = serde_json::from_value(serde_json::json!({
            "model": "gpt-image-1",
            "prompt": "A small red cube",
            "size": "1024x1024"
        }))
        .expect("request");

        request
            .reject_unsupported()
            .expect("image request supported");

        assert_eq!(request.model, "gpt-image-1");
        assert_eq!(request.prompt, "A small red cube");
        assert_eq!(request.size.as_deref(), Some("1024x1024"));
        assert_eq!(request.provider, None);
    }

    #[test]
    fn openai_image_generation_body_accepts_explicit_openai_provider() {
        let request: CloudImageGenerationBody = serde_json::from_value(serde_json::json!({
            "provider": "openai",
            "model": "gpt-image-1",
            "prompt": "A small red cube"
        }))
        .expect("request");

        request
            .reject_unsupported()
            .expect("image request supported");

        assert_eq!(request.provider, Some(Provider::OpenAI));
    }

    #[test]
    fn image_generation_body_accepts_explicit_gemini_provider() {
        let request: CloudImageGenerationBody = serde_json::from_value(serde_json::json!({
            "provider": "gemini",
            "model": "gemini-2.5-flash-image",
            "prompt": "A small red cube",
            "size": "1024x1024"
        }))
        .expect("request");

        request
            .reject_unsupported()
            .expect("Gemini image generation request supported");

        assert_eq!(request.model, "gemini-2.5-flash-image");
        assert_eq!(request.prompt, "A small red cube");
        assert_eq!(request.size.as_deref(), Some("1024x1024"));
        assert_eq!(request.provider, Some(Provider::Gemini));
    }

    #[test]
    fn image_generation_body_accepts_google_provider_alias() {
        let request: CloudImageGenerationBody = serde_json::from_value(serde_json::json!({
            "provider": "google",
            "model": "gemini-2.5-flash-image",
            "prompt": "A small red cube"
        }))
        .expect("request");

        assert_eq!(request.provider, Some(Provider::Gemini));
    }
}
