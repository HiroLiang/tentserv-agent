//! Auth package ports.

use std::{future::Future, pin::Pin};

use crate::foundation::error::KernelResult;

use super::domain::{
    AuthEnvLoadPolicy, AuthEnvSecretMaterial, AuthProviderMetadata, AuthSecretAccessPolicy,
    AuthSecretMaterial, AuthValidationState, KeychainPresence, Provider,
};

pub type AuthValidationFuture<'a> =
    Pin<Box<dyn Future<Output = KernelResult<AuthValidationState>> + Send + 'a>>;

pub trait AuthEnvSecretProbe {
    fn probe_env_secret(
        &self,
        provider: Provider,
        policy: AuthEnvLoadPolicy,
    ) -> KernelResult<Option<AuthEnvSecretMaterial>>;
}

pub trait AuthKeychainSecretStore {
    fn keychain_presence(&self, provider: Provider) -> KernelResult<KeychainPresence>;

    /// Store implementations own native unlock behavior for secret reads.
    fn read_keychain_secret(
        &self,
        provider: Provider,
        policy: AuthSecretAccessPolicy,
    ) -> KernelResult<Option<String>>;

    fn write_keychain_secret(&self, provider: Provider, secret: &str) -> KernelResult<()>;

    fn remove_keychain_secret(&self, provider: Provider) -> KernelResult<bool>;
}

pub trait AuthSecretValidator {
    fn validate<'a>(&'a self, provider: Provider, secret: &'a str) -> AuthValidationFuture<'a>;
}

pub trait AuthSecretCache {
    fn load_cached_secret(&self, provider: Provider) -> KernelResult<Option<AuthSecretMaterial>>;

    fn save_cached_secret(&self, secret: AuthSecretMaterial) -> KernelResult<()>;

    fn remove_cached_secret(&self, provider: Provider) -> KernelResult<()>;
}

pub trait AuthMetadataStore {
    fn load_provider_metadata(
        &self,
        provider: Provider,
    ) -> KernelResult<Option<AuthProviderMetadata>>;

    fn save_provider_metadata(&self, metadata: &AuthProviderMetadata) -> KernelResult<()>;

    fn remove_provider_metadata(&self, provider: Provider) -> KernelResult<()>;
}
