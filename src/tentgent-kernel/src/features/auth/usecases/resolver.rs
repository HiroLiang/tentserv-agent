//! Auth secret resolution use case.

use crate::features::auth::domain::{
    normalize_secret_value, AuthEnvLoadPolicy, AuthSecretAccessPolicy, AuthSecretMaterial,
    AuthSecretSource, AuthSourceMode, Provider,
};
use crate::features::auth::ports::{
    AuthEnvSecretProbe, AuthKeychainSecretStore, AuthMetadataStore, AuthSecretCache,
};
use crate::foundation::error::{KernelError, KernelResult};

use super::port::AuthSecretResolverUseCase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSecretResolutionRequest {
    pub provider: Provider,
    pub env_policy: AuthEnvLoadPolicy,
    pub access_policy: AuthSecretAccessPolicy,
    pub provided_secret: Option<AuthSecretMaterial>,
}

impl AuthSecretResolutionRequest {
    pub fn for_secret_use(provider: Provider, env_policy: AuthEnvLoadPolicy) -> Self {
        Self {
            provider,
            env_policy,
            access_policy: AuthSecretAccessPolicy::resolve_for_use(),
            provided_secret: None,
        }
    }

    pub fn for_secret_validation(provider: Provider, env_policy: AuthEnvLoadPolicy) -> Self {
        Self {
            provider,
            env_policy,
            access_policy: AuthSecretAccessPolicy::resolve_and_validate(),
            provided_secret: None,
        }
    }

    pub fn with_prompt_secret(mut self, secret: impl Into<String>) -> Self {
        self.provided_secret =
            AuthSecretMaterial::normalized(self.provider, AuthSecretSource::Prompt, secret);
        self
    }

    pub fn with_request_secret(mut self, secret: impl Into<String>) -> Self {
        self.provided_secret =
            AuthSecretMaterial::normalized(self.provider, AuthSecretSource::Request, secret);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSecretResolution {
    pub provider: Provider,
    pub secret: Option<AuthSecretMaterial>,
    pub keychain_read_attempted: bool,
}

impl AuthSecretResolution {
    pub fn source(&self) -> Option<AuthSecretSource> {
        self.secret.as_ref().map(|secret| secret.source)
    }

    pub const fn is_resolved(&self) -> bool {
        self.secret.is_some()
    }
}

pub struct StdAuthSecretResolverUseCase<'a> {
    env_probe: &'a dyn AuthEnvSecretProbe,
    keychain_store: &'a dyn AuthKeychainSecretStore,
    metadata_store: &'a dyn AuthMetadataStore,
    cache: &'a dyn AuthSecretCache,
}

impl<'a> StdAuthSecretResolverUseCase<'a> {
    pub fn new(
        env_probe: &'a dyn AuthEnvSecretProbe,
        keychain_store: &'a dyn AuthKeychainSecretStore,
        metadata_store: &'a dyn AuthMetadataStore,
        cache: &'a dyn AuthSecretCache,
    ) -> Self {
        Self {
            env_probe,
            keychain_store,
            metadata_store,
            cache,
        }
    }
}

impl AuthSecretResolverUseCase for StdAuthSecretResolverUseCase<'_> {
    fn resolve_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution> {
        if let Some(secret) = request.provided_secret {
            if secret.provider != request.provider {
                return Err(KernelError::RuntimeStateUnavailable(format!(
                    "provided auth secret provider {:?} does not match request provider {:?}",
                    secret.provider, request.provider
                )));
            }

            if request.access_policy.should_cache_for_process() {
                self.cache.save_cached_secret(secret.clone())?;
            }

            return Ok(AuthSecretResolution {
                provider: request.provider,
                secret: Some(secret),
                keychain_read_attempted: false,
            });
        }

        let preference = self
            .metadata_store
            .load_provider_preference(request.provider)?;

        match preference.source_mode {
            AuthSourceMode::Auto => {
                if let Some(env_secret) = self
                    .env_probe
                    .probe_env_secret(request.provider, request.env_policy.clone())?
                {
                    return Ok(AuthSecretResolution {
                        provider: request.provider,
                        secret: Some(env_secret.into_secret_material()),
                        keychain_read_attempted: false,
                    });
                }

                if request.access_policy.should_cache_for_process() {
                    if let Some(secret) = self.cache.load_cached_secret(request.provider)? {
                        return Ok(AuthSecretResolution {
                            provider: request.provider,
                            secret: Some(secret),
                            keychain_read_attempted: false,
                        });
                    }
                }

                self.resolve_keychain_secret(request)
            }
            AuthSourceMode::Env => {
                let secret = self
                    .env_probe
                    .probe_env_secret(request.provider, AuthEnvLoadPolicy::ProcessOnly)?
                    .map(|secret| secret.into_secret_material());
                return Ok(AuthSecretResolution {
                    provider: request.provider,
                    secret,
                    keychain_read_attempted: false,
                });
            }
            AuthSourceMode::File => {
                let secret = match preference.env_file {
                    Some(path) => self
                        .env_probe
                        .probe_env_secret(
                            request.provider,
                            AuthEnvLoadPolicy::ExplicitDotenvOverride { path },
                        )?
                        .map(|secret| secret.into_secret_material()),
                    None => None,
                };
                return Ok(AuthSecretResolution {
                    provider: request.provider,
                    secret,
                    keychain_read_attempted: false,
                });
            }
            AuthSourceMode::Keychain => self.resolve_keychain_secret(request),
            AuthSourceMode::None => Ok(AuthSecretResolution {
                provider: request.provider,
                secret: None,
                keychain_read_attempted: false,
            }),
        }
    }
}

impl StdAuthSecretResolverUseCase<'_> {
    fn resolve_keychain_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution> {
        if !request.access_policy.may_read_keychain_secret() {
            return Ok(AuthSecretResolution {
                provider: request.provider,
                secret: None,
                keychain_read_attempted: false,
            });
        }
        let secret = self
            .keychain_store
            .read_keychain_secret(request.provider, request.access_policy)?
            .and_then(normalize_secret_value)
            .map(|secret| {
                AuthSecretMaterial::new(request.provider, AuthSecretSource::Keychain, secret)
            });

        if let Some(secret) = secret.as_ref() {
            if request.access_policy.should_cache_for_process() {
                self.cache.save_cached_secret(secret.clone())?;
            }
        }

        Ok(AuthSecretResolution {
            provider: request.provider,
            secret,
            keychain_read_attempted: true,
        })
    }
}
