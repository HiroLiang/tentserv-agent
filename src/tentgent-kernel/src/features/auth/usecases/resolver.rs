//! Auth secret resolution use case.

use crate::features::auth::domain::{
    normalize_secret_value, AuthEnvLoadPolicy, AuthSecretAccessPolicy, AuthSecretMaterial,
    AuthSecretSource, KeychainPromptPlan, Provider,
};
use crate::features::auth::ports::{
    AuthEnvSecretProbe, AuthKeychainSecretStore, AuthSecretCache, KeychainPromptPlanner,
};
use crate::foundation::error::KernelResult;
use crate::foundation::platform::PlatformFacts;

use super::port::AuthSecretResolverUseCase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSecretResolutionRequest {
    pub provider: Provider,
    pub env_policy: AuthEnvLoadPolicy,
    pub access_policy: AuthSecretAccessPolicy,
    pub platform: PlatformFacts,
}

impl AuthSecretResolutionRequest {
    pub fn cli_use(
        provider: Provider,
        env_policy: AuthEnvLoadPolicy,
        platform: PlatformFacts,
    ) -> Self {
        Self {
            provider,
            env_policy,
            access_policy: AuthSecretAccessPolicy::cli_secret_use(),
            platform,
        }
    }

    pub fn cli_validation(
        provider: Provider,
        env_policy: AuthEnvLoadPolicy,
        platform: PlatformFacts,
    ) -> Self {
        Self {
            provider,
            env_policy,
            access_policy: AuthSecretAccessPolicy::cli_secret_validation(),
            platform,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSecretResolution {
    pub provider: Provider,
    pub secret: Option<AuthSecretMaterial>,
    pub prompt_plan: Option<KeychainPromptPlan>,
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
    cache: &'a dyn AuthSecretCache,
    prompt_planner: &'a dyn KeychainPromptPlanner,
}

impl<'a> StdAuthSecretResolverUseCase<'a> {
    pub fn new(
        env_probe: &'a dyn AuthEnvSecretProbe,
        keychain_store: &'a dyn AuthKeychainSecretStore,
        cache: &'a dyn AuthSecretCache,
        prompt_planner: &'a dyn KeychainPromptPlanner,
    ) -> Self {
        Self {
            env_probe,
            keychain_store,
            cache,
            prompt_planner,
        }
    }
}

impl AuthSecretResolverUseCase for StdAuthSecretResolverUseCase<'_> {
    fn resolve_secret(
        &self,
        request: AuthSecretResolutionRequest,
    ) -> KernelResult<AuthSecretResolution> {
        if let Some(env_secret) = self
            .env_probe
            .probe_env_secret(request.provider, request.env_policy)?
        {
            return Ok(AuthSecretResolution {
                provider: request.provider,
                secret: Some(env_secret.into_secret_material()),
                prompt_plan: None,
                keychain_read_attempted: false,
            });
        }

        if request.access_policy.should_cache_for_process() {
            if let Some(secret) = self.cache.load_cached_secret(request.provider)? {
                return Ok(AuthSecretResolution {
                    provider: request.provider,
                    secret: Some(secret),
                    prompt_plan: None,
                    keychain_read_attempted: false,
                });
            }
        }

        if !request.access_policy.may_read_keychain_secret() {
            return Ok(AuthSecretResolution {
                provider: request.provider,
                secret: None,
                prompt_plan: None,
                keychain_read_attempted: false,
            });
        }

        let prompt_plan = self
            .prompt_planner
            .plan_prompt(&request.platform, request.access_policy.keychain_prompt)?;
        let keychain_policy = AuthSecretAccessPolicy {
            keychain_prompt: prompt_plan.effective,
            ..request.access_policy
        };
        let secret = self
            .keychain_store
            .read_keychain_secret(request.provider, keychain_policy)?
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
            prompt_plan: Some(prompt_plan),
            keychain_read_attempted: true,
        })
    }
}
