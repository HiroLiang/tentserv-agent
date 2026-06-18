//! Auth package ports.

use std::{future::Future, pin::Pin};

use crate::foundation::error::KernelResult;

use super::domain::{
    AuthEnvLoadPolicy, AuthEnvSecretMaterial, AuthProviderMetadata, AuthProviderPreference,
    AuthSecretAccessPolicy, AuthSecretMaterial, AuthValidationState, KeychainPresence, Provider,
};

pub type AuthValidationFuture<'a> =
    Pin<Box<dyn Future<Output = KernelResult<AuthValidationState>> + Send + 'a>>;

/// Reads provider secrets from process environment or `.env` material according to policy.
pub trait AuthEnvSecretProbe {
    /// Returns normalized secret material and origin metadata without persisting it.
    fn probe_env_secret(
        &self,
        provider: Provider,
        policy: AuthEnvLoadPolicy,
    ) -> KernelResult<Option<AuthEnvSecretMaterial>>;
}

/// Stores provider secrets in the native operating-system credential store.
pub trait AuthKeychainSecretStore {
    /// Checks whether a provider secret is present without exposing the secret value to callers.
    fn keychain_presence(&self, provider: Provider) -> KernelResult<KeychainPresence>;

    /// Store implementations own native unlock behavior for secret reads.
    fn read_keychain_secret(
        &self,
        provider: Provider,
        policy: AuthSecretAccessPolicy,
    ) -> KernelResult<Option<String>>;

    /// Writes or replaces the provider secret in native secure storage.
    fn write_keychain_secret(&self, provider: Provider, secret: &str) -> KernelResult<()>;

    /// Removes the provider secret and reports whether an entry was removed.
    fn remove_keychain_secret(&self, provider: Provider) -> KernelResult<bool>;
}

/// Validates provider secrets against the provider-facing authentication endpoint.
pub trait AuthSecretValidator {
    /// Performs an async validation request and maps the provider response to domain state.
    fn validate<'a>(&'a self, provider: Provider, secret: &'a str) -> AuthValidationFuture<'a>;
}

/// Keeps short-lived provider secrets in process memory for repeated local operations.
pub trait AuthSecretCache {
    /// Loads a cached provider secret if it exists and is still valid.
    fn load_cached_secret(&self, provider: Provider) -> KernelResult<Option<AuthSecretMaterial>>;

    /// Saves a provider secret for the cache lifetime owned by the implementation.
    fn save_cached_secret(&self, secret: AuthSecretMaterial) -> KernelResult<()>;

    /// Removes a cached provider secret from process memory.
    fn remove_cached_secret(&self, provider: Provider) -> KernelResult<()>;
}

/// Persists non-secret provider auth metadata such as presence and validation state.
pub trait AuthMetadataStore {
    /// Loads previously recorded non-secret metadata for a provider.
    fn load_provider_metadata(
        &self,
        provider: Provider,
    ) -> KernelResult<Option<AuthProviderMetadata>>;

    /// Saves non-secret metadata; implementations must never serialize secret material.
    fn save_provider_metadata(&self, metadata: &AuthProviderMetadata) -> KernelResult<()>;

    /// Removes recorded non-secret metadata for a provider.
    fn remove_provider_metadata(&self, provider: Provider) -> KernelResult<()>;

    /// Loads non-secret provider auth preference, defaulting when no preference is recorded.
    fn load_provider_preference(&self, provider: Provider) -> KernelResult<AuthProviderPreference>;

    /// Saves non-secret provider auth preference. Implementations must never serialize secrets.
    fn save_provider_preference(&self, preference: &AuthProviderPreference) -> KernelResult<()>;
}
