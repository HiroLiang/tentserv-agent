//! Reqwest-backed provider auth secret validator.

use std::time::Duration;

use reqwest::{Client, Method, Request, StatusCode, Url};

use crate::features::auth::domain::{AuthValidationState, Provider};
use crate::features::auth::ports::{AuthSecretValidator, AuthValidationFuture};
use crate::foundation::error::{KernelError, KernelResult};

const VALIDATION_TIMEOUT: Duration = Duration::from_secs(10);
const HUGGINGFACE_VALIDATION_URL: &str = "https://huggingface.co/api/whoami-v2";
const OPENAI_VALIDATION_URL: &str = "https://api.openai.com/v1/models";
const ANTHROPIC_VALIDATION_URL: &str = "https://api.anthropic.com/v1/models";
const GEMINI_VALIDATION_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
const ANTHROPIC_API_KEY_HEADER: &str = "x-api-key";

#[derive(Debug, Clone)]
pub struct ReqwestAuthSecretValidator {
    client: Client,
}

impl ReqwestAuthSecretValidator {
    pub fn new() -> KernelResult<Self> {
        let client = Client::builder()
            .timeout(VALIDATION_TIMEOUT)
            .build()
            .map_err(|err| {
                KernelError::RuntimeStateUnavailable(format!(
                    "failed to build auth validation HTTP client: {err}"
                ))
            })?;

        Ok(Self { client })
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    #[cfg(test)]
    pub(crate) fn validation_request(provider: Provider, secret: &str) -> KernelResult<Request> {
        let client = Client::new();
        validation_request_with_client(&client, provider, secret)
    }

    pub(crate) fn validation_state_for_status(
        provider: Provider,
        status: StatusCode,
    ) -> AuthValidationState {
        match status {
            status if status.is_success() => AuthValidationState::Verified,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => AuthValidationState::Invalid {
                reason: format!("{} rejected the provided API key.", provider.display_name()),
            },
            status => AuthValidationState::Unknown {
                reason: format!(
                    "{} returned HTTP {} during validation.",
                    provider.display_name(),
                    status.as_u16()
                ),
            },
        }
    }
}

impl AuthSecretValidator for ReqwestAuthSecretValidator {
    fn validate<'a>(&'a self, provider: Provider, secret: &'a str) -> AuthValidationFuture<'a> {
        Box::pin(async move {
            if secret.trim().is_empty() {
                return Ok(AuthValidationState::Missing);
            }

            let request = validation_request_with_client(&self.client, provider, secret)?;
            let response = self.client.execute(request).await;
            Ok(validation_state_for_response(provider, response))
        })
    }
}

fn validation_request_with_client(
    client: &Client,
    provider: Provider,
    secret: &str,
) -> KernelResult<Request> {
    let request = match provider {
        Provider::HuggingFace => client
            .request(Method::GET, HUGGINGFACE_VALIDATION_URL)
            .bearer_auth(secret),
        Provider::OpenAI => client
            .request(Method::GET, OPENAI_VALIDATION_URL)
            .bearer_auth(secret),
        Provider::Anthropic => client
            .request(Method::GET, ANTHROPIC_VALIDATION_URL)
            .header(ANTHROPIC_API_KEY_HEADER, secret)
            .header(ANTHROPIC_VERSION_HEADER, ANTHROPIC_VERSION),
        Provider::Gemini => client.request(Method::GET, gemini_validation_url(secret)?),
    };

    request.build().map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to build {} auth validation request: {err}",
            provider.display_name()
        ))
    })
}

fn gemini_validation_url(secret: &str) -> KernelResult<Url> {
    let mut url = Url::parse(GEMINI_VALIDATION_URL).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to parse Gemini auth validation URL: {err}"
        ))
    })?;
    url.query_pairs_mut().append_pair("key", secret);
    Ok(url)
}

fn validation_state_for_response(
    provider: Provider,
    response: Result<reqwest::Response, reqwest::Error>,
) -> AuthValidationState {
    match response {
        Ok(response) => {
            ReqwestAuthSecretValidator::validation_state_for_status(provider, response.status())
        }
        Err(err) => AuthValidationState::Unknown {
            reason: format!("Unable to validate {} key: {err}", provider.display_name()),
        },
    }
}
