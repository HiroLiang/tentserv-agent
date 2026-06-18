//! Auth use case implementations.

pub mod mutation;
pub mod port;
pub mod preference;
pub mod resolver;
pub mod status;
pub mod validation;

#[cfg(test)]
mod tests;

pub use mutation::{
    RemoveAuthSecretRequest, RemoveAuthSecretResult, SetAuthSecretRequest, SetAuthSecretResult,
    StdAuthSecretMutationUseCase,
};
pub use port::{
    AuthPreferenceUseCase, AuthSecretMutationUseCase, AuthSecretResolverUseCase,
    AuthSecretValidationUseCase, AuthStatusUseCase, AuthUseCaseFuture,
};
pub use preference::{
    AuthPreferenceListRequest, AuthPreferenceReport, AuthPreferenceRequest,
    SetAuthPreferenceRequest, StdAuthPreferenceUseCase,
};
pub use resolver::{
    AuthSecretResolution, AuthSecretResolutionRequest, StdAuthSecretResolverUseCase,
};
pub use status::{AuthStatusReport, AuthStatusRequest, StdAuthStatusUseCase};
pub use validation::{
    AuthSecretValidationRequest, AuthSecretValidationResult, StdAuthSecretValidationUseCase,
};
