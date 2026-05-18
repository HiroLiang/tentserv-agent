use axum::{
    extract::{Path, State},
    Json,
};
use tentgent_kernel::features::auth::{
    domain::{
        AuthEnvLoadPolicy, AuthKeyStatus, AuthSecretSource, AuthValidationState, KeychainPresence,
        Provider,
    },
    usecases::{AuthStatusRequest, AuthStatusUseCase},
};

use crate::transport::rest::{error::RestError, state::RestState};

use super::dto::{
    AuthProviderItem, AuthProviderResponse, AuthProvidersResponse, AuthValidationItem,
};

pub async fn list(
    State(state): State<RestState>,
) -> Result<Json<AuthProvidersResponse>, RestError> {
    let result = state
        .app()
        .services()
        .kernel()
        .auth()
        .status_usecase()
        .status(AuthStatusRequest::all(AuthEnvLoadPolicy::CwdDotenvOverride))
        .map_err(|err| RestError::kernel("auth_status_failed", err))?;

    Ok(Json(AuthProvidersResponse {
        providers: result
            .statuses
            .into_iter()
            .map(auth_provider_item)
            .collect(),
    }))
}

pub async fn inspect(
    State(state): State<RestState>,
    Path(provider_id): Path<String>,
) -> Result<Json<AuthProviderResponse>, RestError> {
    let provider = parse_provider(&provider_id)?;
    let result = state
        .app()
        .services()
        .kernel()
        .auth()
        .status_usecase()
        .status(AuthStatusRequest::for_provider(
            provider,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        ))
        .map_err(|err| RestError::kernel("auth_status_failed", err))?;
    let status = result
        .status_for(provider)
        .cloned()
        .ok_or_else(|| RestError::internal("auth_status_failed", "auth provider status missing"))?;

    Ok(Json(AuthProviderResponse {
        provider: auth_provider_item(status),
    }))
}

fn parse_provider(value: &str) -> Result<Provider, RestError> {
    Provider::ALL
        .into_iter()
        .find(|provider| provider.cli_name() == value)
        .ok_or_else(|| {
            RestError::bad_request(
                "bad_request",
                format!(
                    "invalid auth provider `{value}`; expected one of hf, openai, anthropic, gemini"
                ),
            )
        })
}

fn auth_provider_item(status: AuthKeyStatus) -> AuthProviderItem {
    AuthProviderItem {
        provider: status.provider.cli_name().to_string(),
        display_name: status.provider.display_name().to_string(),
        env_present: status.env_present,
        keychain_present: matches!(status.keychain_presence, KeychainPresence::Present),
        effective_source: status.effective_source.map(auth_secret_source_name),
        validation: auth_validation_item(status.validation),
    }
}

fn auth_secret_source_name(source: AuthSecretSource) -> String {
    match source {
        AuthSecretSource::Env => "env",
        AuthSecretSource::Keychain => "keychain",
        AuthSecretSource::Prompt => "prompt",
        AuthSecretSource::Request => "request",
    }
    .to_string()
}

fn auth_validation_item(validation: AuthValidationState) -> AuthValidationItem {
    let state = match &validation {
        AuthValidationState::Missing => "missing",
        AuthValidationState::NotChecked => "not_checked",
        AuthValidationState::Verified => "verified",
        AuthValidationState::Invalid { .. } => "invalid",
        AuthValidationState::Unknown { .. } => "unknown",
    };
    AuthValidationItem {
        state: state.to_string(),
        summary: validation.summary().to_string(),
        detail: validation.detail().map(str::to_string),
    }
}
