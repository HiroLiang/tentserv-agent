//! Auth status use case.

use crate::features::auth::domain::{
    effective_source, AuthEnvLoadPolicy, AuthKeyStatus, AuthProviderMetadata, AuthValidationState,
    KeychainPresence, Provider,
};
use crate::features::auth::ports::{
    AuthEnvSecretProbe, AuthKeychainSecretStore, AuthMetadataStore,
};
use crate::foundation::error::KernelResult;

use super::port::AuthStatusUseCase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatusRequest {
    pub providers: Vec<Provider>,
    pub env_policy: AuthEnvLoadPolicy,
    pub probe_keychain_presence: bool,
}

impl AuthStatusRequest {
    pub fn all(env_policy: AuthEnvLoadPolicy) -> Self {
        Self {
            providers: Provider::ALL.to_vec(),
            env_policy,
            probe_keychain_presence: false,
        }
    }

    pub fn for_provider(provider: Provider, env_policy: AuthEnvLoadPolicy) -> Self {
        Self {
            providers: vec![provider],
            env_policy,
            probe_keychain_presence: false,
        }
    }

    pub fn with_keychain_probe(mut self) -> Self {
        self.probe_keychain_presence = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatusReport {
    pub statuses: Vec<AuthKeyStatus>,
}

impl AuthStatusReport {
    pub fn status_for(&self, provider: Provider) -> Option<&AuthKeyStatus> {
        self.statuses
            .iter()
            .find(|status| status.provider == provider)
    }
}

pub struct StdAuthStatusUseCase<'a> {
    env_probe: &'a dyn AuthEnvSecretProbe,
    keychain_store: &'a dyn AuthKeychainSecretStore,
    metadata_store: &'a dyn AuthMetadataStore,
}

impl<'a> StdAuthStatusUseCase<'a> {
    pub fn new(
        env_probe: &'a dyn AuthEnvSecretProbe,
        keychain_store: &'a dyn AuthKeychainSecretStore,
        metadata_store: &'a dyn AuthMetadataStore,
    ) -> Self {
        Self {
            env_probe,
            keychain_store,
            metadata_store,
        }
    }
}

impl AuthStatusUseCase for StdAuthStatusUseCase<'_> {
    fn status(&self, request: AuthStatusRequest) -> KernelResult<AuthStatusReport> {
        let mut statuses = Vec::with_capacity(request.providers.len());

        for provider in request.providers {
            let env_secret = self
                .env_probe
                .probe_env_secret(provider, request.env_policy.clone())?;
            let metadata = self.metadata_store.load_provider_metadata(provider)?;
            let keychain_presence = if request.probe_keychain_presence {
                let presence = self.keychain_store.keychain_presence(provider)?;
                self.save_keychain_presence(provider, presence, metadata.as_ref())?;
                presence
            } else {
                metadata
                    .as_ref()
                    .map(|metadata| metadata.keychain_presence)
                    .unwrap_or(KeychainPresence::Unknown)
            };
            let env_present = env_secret.is_some();
            let effective_source = effective_source(env_present, keychain_presence);
            let validation = status_validation(effective_source.is_some(), metadata.as_ref());

            statuses.push(AuthKeyStatus {
                provider,
                env_present,
                keychain_presence,
                effective_source,
                validation,
            });
        }

        Ok(AuthStatusReport { statuses })
    }
}

impl StdAuthStatusUseCase<'_> {
    fn save_keychain_presence(
        &self,
        provider: Provider,
        keychain_presence: KeychainPresence,
        existing: Option<&AuthProviderMetadata>,
    ) -> KernelResult<()> {
        let metadata = AuthProviderMetadata {
            provider,
            keychain_presence,
            last_updated_at: existing.and_then(|metadata| metadata.last_updated_at.clone()),
            last_validated_at: existing.and_then(|metadata| metadata.last_validated_at.clone()),
            validation: existing
                .map(|metadata| metadata.validation.clone())
                .unwrap_or(AuthValidationState::NotChecked),
        };

        self.metadata_store.save_provider_metadata(&metadata)
    }
}

fn status_validation(
    has_effective_secret: bool,
    metadata: Option<&AuthProviderMetadata>,
) -> AuthValidationState {
    if !has_effective_secret {
        return AuthValidationState::Missing;
    }

    match metadata.map(|metadata| metadata.validation.clone()) {
        Some(AuthValidationState::Missing) | None => AuthValidationState::NotChecked,
        Some(validation) => validation,
    }
}
