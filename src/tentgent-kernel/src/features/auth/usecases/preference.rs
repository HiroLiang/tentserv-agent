//! Auth provider preference use case.

use crate::features::auth::domain::{AuthProviderPreference, AuthSourceMode, Provider};
use crate::features::auth::ports::AuthMetadataStore;
use crate::foundation::error::KernelResult;

use super::port::AuthPreferenceUseCase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPreferenceRequest {
    pub provider: Provider,
}

impl AuthPreferenceRequest {
    pub const fn new(provider: Provider) -> Self {
        Self { provider }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPreferenceListRequest {
    pub providers: Vec<Provider>,
}

impl AuthPreferenceListRequest {
    pub fn all() -> Self {
        Self {
            providers: Provider::ALL.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetAuthPreferenceRequest {
    pub provider: Provider,
    pub source_mode: AuthSourceMode,
    pub env_file: Option<std::path::PathBuf>,
}

impl SetAuthPreferenceRequest {
    pub fn new(provider: Provider, source_mode: AuthSourceMode) -> Self {
        Self {
            provider,
            source_mode,
            env_file: None,
        }
    }

    pub fn with_env_file(mut self, env_file: impl Into<std::path::PathBuf>) -> Self {
        self.env_file = Some(env_file.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPreferenceReport {
    pub preferences: Vec<AuthProviderPreference>,
}

pub struct StdAuthPreferenceUseCase<'a> {
    metadata_store: &'a dyn AuthMetadataStore,
}

impl<'a> StdAuthPreferenceUseCase<'a> {
    pub fn new(metadata_store: &'a dyn AuthMetadataStore) -> Self {
        Self { metadata_store }
    }
}

impl AuthPreferenceUseCase for StdAuthPreferenceUseCase<'_> {
    fn get_preference(
        &self,
        request: AuthPreferenceRequest,
    ) -> KernelResult<AuthProviderPreference> {
        self.metadata_store
            .load_provider_preference(request.provider)
    }

    fn list_preferences(
        &self,
        request: AuthPreferenceListRequest,
    ) -> KernelResult<AuthPreferenceReport> {
        let mut preferences = Vec::with_capacity(request.providers.len());
        for provider in request.providers {
            preferences.push(self.metadata_store.load_provider_preference(provider)?);
        }
        Ok(AuthPreferenceReport { preferences })
    }

    fn set_preference(
        &self,
        request: SetAuthPreferenceRequest,
    ) -> KernelResult<AuthProviderPreference> {
        let preference = AuthProviderPreference {
            provider: request.provider,
            source_mode: request.source_mode,
            env_file: request.env_file,
        };
        self.metadata_store.save_provider_preference(&preference)?;
        Ok(preference)
    }
}
