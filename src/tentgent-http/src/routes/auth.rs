use tentgent_core::auth::{
    AuthError, AuthManager, KeySource, KeyStatus, KeyValidationState, Provider,
};

use crate::{
    dto::{
        AuthProviderItem, AuthProviderResponse, AuthProvidersResponse, AuthValidationItem,
        ErrorResponse,
    },
    http::HttpResponse,
    response::{bad_request_response, json_response},
};

pub(crate) fn auth_providers_response() -> HttpResponse {
    let auth = match AuthManager::new() {
        Ok(auth) => auth,
        Err(error) => return auth_error_response(error),
    };

    let mut providers = Vec::new();
    for provider in Provider::ALL {
        match auth.local_key_status(provider) {
            Ok(status) => providers.push(auth_provider_item(status)),
            Err(error) => return auth_error_response(error),
        }
    }

    json_response(200, AuthProvidersResponse { providers })
}

pub(crate) fn auth_provider_response(provider_id: &str) -> HttpResponse {
    let Some(provider) = parse_provider(provider_id) else {
        return bad_request_response(format!(
            "invalid auth provider `{provider_id}`; expected one of hf, openai, anthropic, gemini"
        ));
    };
    let auth = match AuthManager::new() {
        Ok(auth) => auth,
        Err(error) => return auth_error_response(error),
    };

    match auth.local_key_status(provider) {
        Ok(status) => json_response(
            200,
            AuthProviderResponse {
                provider: auth_provider_item(status),
            },
        ),
        Err(error) => auth_error_response(error),
    }
}

fn parse_provider(value: &str) -> Option<Provider> {
    Provider::ALL
        .into_iter()
        .find(|provider| provider.cli_name() == value)
}

fn auth_provider_item(status: KeyStatus) -> AuthProviderItem {
    AuthProviderItem {
        provider: status.provider.cli_name().to_string(),
        display_name: status.provider.display_name().to_string(),
        env_present: status.env_present,
        keychain_present: status.keychain_present,
        effective_source: status.effective_source.map(key_source_name),
        validation: auth_validation_item(status.validation),
    }
}

fn key_source_name(source: KeySource) -> String {
    match source {
        KeySource::Env => "env".to_string(),
        KeySource::Keychain => "keychain".to_string(),
    }
}

fn auth_validation_item(validation: KeyValidationState) -> AuthValidationItem {
    let state = match &validation {
        KeyValidationState::Missing => "missing",
        KeyValidationState::NotChecked => "not_checked",
        KeyValidationState::Verified => "verified",
        KeyValidationState::Invalid { .. } => "invalid",
        KeyValidationState::Unknown { .. } => "unknown",
    };
    AuthValidationItem {
        state: state.to_string(),
        summary: validation.summary().to_string(),
        detail: validation.detail().map(str::to_string),
    }
}

fn auth_error_response(error: AuthError) -> HttpResponse {
    json_response(
        500,
        ErrorResponse {
            error: "auth_status_failed",
            message: format!("failed to read auth status: {error}"),
        },
    )
}
