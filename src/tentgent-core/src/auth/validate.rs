use std::time::Duration;

use reqwest::{Client, StatusCode};

use super::{AuthError, Provider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyValidationState {
    Missing,
    NotChecked,
    Verified,
    Invalid { reason: String },
    Unknown { reason: String },
}

impl KeyValidationState {
    pub fn summary(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::NotChecked => "not checked",
            Self::Verified => "verified",
            Self::Invalid { .. } => "invalid",
            Self::Unknown { .. } => "unknown",
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Invalid { reason } | Self::Unknown { reason } => Some(reason.as_str()),
            Self::Missing | Self::NotChecked | Self::Verified => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct KeyValidator {
    client: Client,
}

impl KeyValidator {
    pub(crate) fn new() -> Result<Self, AuthError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|err| AuthError::HttpClient(err.to_string()))?;

        Ok(Self { client })
    }

    pub(crate) async fn validate(&self, provider: Provider, secret: &str) -> KeyValidationState {
        match provider {
            Provider::HuggingFace => self.validate_huggingface(secret).await,
            Provider::OpenAI => self.validate_openai(secret).await,
            Provider::Anthropic => self.validate_anthropic(secret).await,
        }
    }

    async fn validate_huggingface(&self, secret: &str) -> KeyValidationState {
        self.validate_bearer(
            "https://huggingface.co/api/whoami-v2",
            secret,
            "Hugging Face",
        )
        .await
    }

    async fn validate_openai(&self, secret: &str) -> KeyValidationState {
        self.validate_bearer("https://api.openai.com/v1/models", secret, "OpenAI")
            .await
    }

    async fn validate_anthropic(&self, secret: &str) -> KeyValidationState {
        let response = self
            .client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", secret)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await;

        Self::map_response("Anthropic", response)
    }

    async fn validate_bearer(
        &self,
        url: &str,
        secret: &str,
        provider_name: &str,
    ) -> KeyValidationState {
        let response = self.client.get(url).bearer_auth(secret).send().await;
        Self::map_response(provider_name, response)
    }

    fn map_response(
        provider_name: &str,
        response: Result<reqwest::Response, reqwest::Error>,
    ) -> KeyValidationState {
        match response {
            Ok(response) if response.status().is_success() => KeyValidationState::Verified,
            Ok(response)
                if response.status() == StatusCode::UNAUTHORIZED
                    || response.status() == StatusCode::FORBIDDEN =>
            {
                KeyValidationState::Invalid {
                    reason: format!("{provider_name} rejected the provided API key."),
                }
            }
            Ok(response) => KeyValidationState::Unknown {
                reason: format!(
                    "{provider_name} returned HTTP {} during validation.",
                    response.status().as_u16()
                ),
            },
            Err(err) => KeyValidationState::Unknown {
                reason: format!("Unable to validate key: {err}"),
            },
        }
    }
}
