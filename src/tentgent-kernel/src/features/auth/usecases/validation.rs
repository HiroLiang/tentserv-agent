//! Auth secret validation use case.

use crate::features::auth::domain::{
    AuthProviderMetadata, AuthSecretSource, AuthValidationState, KeychainPresence,
    KeychainPromptPlan, Provider,
};
use crate::features::auth::ports::{AuthMetadataStore, AuthSecretValidator};

use super::port::{AuthSecretResolverUseCase, AuthSecretValidationUseCase, AuthUseCaseFuture};
use super::resolver::AuthSecretResolutionRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSecretValidationRequest {
    pub resolution: AuthSecretResolutionRequest,
    pub validated_at: Option<String>,
}

impl AuthSecretValidationRequest {
    pub fn new(resolution: AuthSecretResolutionRequest) -> Self {
        Self {
            resolution,
            validated_at: None,
        }
    }

    pub fn with_validated_at(mut self, validated_at: impl Into<String>) -> Self {
        self.validated_at = Some(validated_at.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSecretValidationResult {
    pub provider: Provider,
    pub source: Option<AuthSecretSource>,
    pub prompt_plan: Option<KeychainPromptPlan>,
    pub validation: AuthValidationState,
}

pub struct StdAuthSecretValidationUseCase<'a> {
    resolver: &'a dyn AuthSecretResolverUseCase,
    validator: &'a dyn AuthSecretValidator,
    metadata_store: &'a dyn AuthMetadataStore,
}

impl<'a> StdAuthSecretValidationUseCase<'a> {
    pub fn new(
        resolver: &'a dyn AuthSecretResolverUseCase,
        validator: &'a dyn AuthSecretValidator,
        metadata_store: &'a dyn AuthMetadataStore,
    ) -> Self {
        Self {
            resolver,
            validator,
            metadata_store,
        }
    }
}

impl AuthSecretValidationUseCase for StdAuthSecretValidationUseCase<'_> {
    fn validate_secret<'a>(
        &'a self,
        request: AuthSecretValidationRequest,
    ) -> AuthUseCaseFuture<'a, AuthSecretValidationResult> {
        Box::pin(async move {
            let resolution = self.resolver.resolve_secret(request.resolution)?;
            let provider = resolution.provider;
            let existing = self.metadata_store.load_provider_metadata(provider)?;
            let validation = match resolution.secret.as_ref() {
                Some(secret) => self.validator.validate(provider, secret.secret()).await?,
                None => AuthValidationState::Missing,
            };
            let keychain_presence =
                keychain_presence_after_validation(&resolution, existing.as_ref());
            let metadata = AuthProviderMetadata {
                provider,
                keychain_presence,
                last_updated_at: existing
                    .as_ref()
                    .and_then(|metadata| metadata.last_updated_at.clone()),
                last_validated_at: request.validated_at,
                validation: validation.clone(),
            };
            self.metadata_store.save_provider_metadata(&metadata)?;

            Ok(AuthSecretValidationResult {
                provider,
                source: resolution.source(),
                prompt_plan: resolution.prompt_plan,
                validation,
            })
        })
    }
}

fn keychain_presence_after_validation(
    resolution: &super::resolver::AuthSecretResolution,
    existing: Option<&AuthProviderMetadata>,
) -> KeychainPresence {
    match resolution.source() {
        Some(AuthSecretSource::Keychain) => KeychainPresence::Present,
        Some(AuthSecretSource::Env) => existing
            .map(|metadata| metadata.keychain_presence)
            .unwrap_or(KeychainPresence::Unknown),
        None if resolution.keychain_read_attempted => KeychainPresence::Absent,
        None => existing
            .map(|metadata| metadata.keychain_presence)
            .unwrap_or(KeychainPresence::Unknown),
    }
}
