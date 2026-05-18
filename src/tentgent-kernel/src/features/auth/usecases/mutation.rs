//! Auth secret mutation use case.

use crate::features::auth::domain::{
    AuthProviderMetadata, AuthSecretCacheScope, AuthSecretMaterial, AuthSecretSource,
    AuthValidationState, KeychainPresence, Provider,
};
use crate::features::auth::ports::{AuthKeychainSecretStore, AuthMetadataStore, AuthSecretCache};
use crate::foundation::error::KernelResult;
use zeroize::Zeroizing;

use super::port::AuthSecretMutationUseCase;

#[derive(Clone, PartialEq, Eq)]
pub struct SetAuthSecretRequest {
    pub provider: Provider,
    pub secret: Zeroizing<String>,
    pub cache_scope: AuthSecretCacheScope,
    pub updated_at: Option<String>,
    pub validation: AuthValidationState,
}

impl SetAuthSecretRequest {
    pub fn new(provider: Provider, secret: impl Into<String>) -> Self {
        Self {
            provider,
            secret: Zeroizing::new(secret.into()),
            cache_scope: AuthSecretCacheScope::ProcessSession,
            updated_at: None,
            validation: AuthValidationState::NotChecked,
        }
    }

    pub fn without_cache(mut self) -> Self {
        self.cache_scope = AuthSecretCacheScope::None;
        self
    }

    pub fn with_updated_at(mut self, updated_at: impl Into<String>) -> Self {
        self.updated_at = Some(updated_at.into());
        self
    }

    pub fn with_validation(mut self, validation: AuthValidationState) -> Self {
        self.validation = validation;
        self
    }
}

impl std::fmt::Debug for SetAuthSecretRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SetAuthSecretRequest")
            .field("provider", &self.provider)
            .field("secret", &"<redacted>")
            .field("cache_scope", &self.cache_scope)
            .field("updated_at", &self.updated_at)
            .field("validation", &self.validation)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetAuthSecretResult {
    pub provider: Provider,
    pub keychain_presence: KeychainPresence,
    pub validation: AuthValidationState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveAuthSecretRequest {
    pub provider: Provider,
}

impl RemoveAuthSecretRequest {
    pub const fn new(provider: Provider) -> Self {
        Self { provider }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveAuthSecretResult {
    pub provider: Provider,
    pub removed: bool,
    pub keychain_presence: KeychainPresence,
}

pub struct StdAuthSecretMutationUseCase<'a> {
    keychain_store: &'a dyn AuthKeychainSecretStore,
    metadata_store: &'a dyn AuthMetadataStore,
    cache: &'a dyn AuthSecretCache,
}

impl<'a> StdAuthSecretMutationUseCase<'a> {
    pub fn new(
        keychain_store: &'a dyn AuthKeychainSecretStore,
        metadata_store: &'a dyn AuthMetadataStore,
        cache: &'a dyn AuthSecretCache,
    ) -> Self {
        Self {
            keychain_store,
            metadata_store,
            cache,
        }
    }
}

impl AuthSecretMutationUseCase for StdAuthSecretMutationUseCase<'_> {
    fn set_secret(&self, request: SetAuthSecretRequest) -> KernelResult<SetAuthSecretResult> {
        self.keychain_store
            .write_keychain_secret(request.provider, request.secret.as_str())?;

        let metadata = AuthProviderMetadata {
            provider: request.provider,
            keychain_presence: KeychainPresence::Present,
            last_updated_at: request.updated_at,
            last_validated_at: None,
            validation: request.validation.clone(),
        };
        self.metadata_store.save_provider_metadata(&metadata)?;

        if request.cache_scope == AuthSecretCacheScope::ProcessSession {
            self.cache.save_cached_secret(AuthSecretMaterial::new(
                request.provider,
                AuthSecretSource::Keychain,
                request.secret.to_string(),
            ))?;
        } else {
            self.cache.remove_cached_secret(request.provider)?;
        }

        Ok(SetAuthSecretResult {
            provider: request.provider,
            keychain_presence: KeychainPresence::Present,
            validation: request.validation,
        })
    }

    fn remove_secret(
        &self,
        request: RemoveAuthSecretRequest,
    ) -> KernelResult<RemoveAuthSecretResult> {
        let removed = self
            .keychain_store
            .remove_keychain_secret(request.provider)?;
        self.metadata_store
            .remove_provider_metadata(request.provider)?;
        self.cache.remove_cached_secret(request.provider)?;

        Ok(RemoveAuthSecretResult {
            provider: request.provider,
            removed,
            keychain_presence: KeychainPresence::Absent,
        })
    }
}
