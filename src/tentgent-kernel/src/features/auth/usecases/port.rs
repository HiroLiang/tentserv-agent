//! Auth use case ports.

use std::{future::Future, pin::Pin};

use crate::foundation::error::KernelResult;

use super::mutation::{
    RemoveAuthSecretRequest, RemoveAuthSecretResult, SetAuthSecretRequest, SetAuthSecretResult,
};
use super::preference::{
    AuthPreferenceListRequest, AuthPreferenceReport, AuthPreferenceRequest,
    SetAuthPreferenceRequest,
};
use super::resolver::{AuthSecretResolution, AuthSecretResolutionRequest};
use super::status::{AuthStatusReport, AuthStatusRequest};
use super::validation::{AuthSecretValidationRequest, AuthSecretValidationResult};

/// Boxed async return type used by auth use cases that perform network work.
pub type AuthUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Use-case boundary for assembling non-secret provider auth status.
pub trait AuthStatusUseCase {
    /// Builds a status report without reading secret material unless the request explicitly asks.
    fn status(&self, request: AuthStatusRequest) -> KernelResult<AuthStatusReport>;
}

/// Use-case boundary for resolving the effective provider secret for one operation.
pub trait AuthSecretResolverUseCase {
    /// Resolves explicit request input, environment, process cache, and Keychain according to policy.
    fn resolve_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution>;
}

/// Use-case boundary for local provider secret mutations.
pub trait AuthSecretMutationUseCase {
    /// Stores a provider secret and updates cache and non-secret metadata consistently.
    fn set_secret(&self, request: SetAuthSecretRequest) -> KernelResult<SetAuthSecretResult>;

    /// Removes a provider secret and clears related cache and metadata.
    fn remove_secret(
        &self,
        request: RemoveAuthSecretRequest,
    ) -> KernelResult<RemoveAuthSecretResult>;
}

/// Use-case boundary for non-secret provider auth preferences.
pub trait AuthPreferenceUseCase {
    /// Loads one provider preference, returning the default when unset.
    fn get_preference(
        &self,
        request: AuthPreferenceRequest,
    ) -> KernelResult<crate::features::auth::domain::AuthProviderPreference>;

    /// Loads preferences for a provider set.
    fn list_preferences(
        &self,
        request: AuthPreferenceListRequest,
    ) -> KernelResult<AuthPreferenceReport>;

    /// Saves one non-secret provider auth preference.
    fn set_preference(
        &self,
        request: SetAuthPreferenceRequest,
    ) -> KernelResult<crate::features::auth::domain::AuthProviderPreference>;
}

/// Use-case boundary for resolving and validating provider secrets.
pub trait AuthSecretValidationUseCase {
    /// Resolves a secret, validates it with the provider, and records non-secret validation metadata.
    fn validate_secret<'a>(
        &'a self,
        request: AuthSecretValidationRequest,
    ) -> AuthUseCaseFuture<'a, AuthSecretValidationResult>;
}
