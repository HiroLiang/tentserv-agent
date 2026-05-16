//! Auth use case ports.

use std::{future::Future, pin::Pin};

use crate::foundation::error::KernelResult;

use super::mutation::{
    RemoveAuthSecretRequest, RemoveAuthSecretResult, SetAuthSecretRequest, SetAuthSecretResult,
};
use super::resolver::{AuthSecretResolution, AuthSecretResolutionRequest};
use super::status::{AuthStatusReport, AuthStatusRequest};
use super::validation::{AuthSecretValidationRequest, AuthSecretValidationResult};

pub type AuthUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

pub trait AuthStatusUseCase {
    fn status(&self, request: AuthStatusRequest) -> KernelResult<AuthStatusReport>;
}

pub trait AuthSecretResolverUseCase {
    fn resolve_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution>;
}

pub trait AuthSecretMutationUseCase {
    fn set_secret(&self, request: SetAuthSecretRequest) -> KernelResult<SetAuthSecretResult>;

    fn remove_secret(
        &self,
        request: RemoveAuthSecretRequest,
    ) -> KernelResult<RemoveAuthSecretResult>;
}

pub trait AuthSecretValidationUseCase {
    fn validate_secret<'a>(
        &'a self,
        request: AuthSecretValidationRequest,
    ) -> AuthUseCaseFuture<'a, AuthSecretValidationResult>;
}
