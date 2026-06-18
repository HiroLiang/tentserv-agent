use std::collections::HashMap;
use std::sync::Mutex;

use crate::features::auth::domain::{
    AuthEnvLoadPolicy, AuthEnvSecretMaterial, AuthEnvSecretOrigin, AuthProviderMetadata,
    AuthProviderPreference, AuthSecretAccessPolicy, AuthSecretMaterial, AuthSecretSource,
    AuthSourceMode, AuthValidationState, KeychainPresence, Provider,
};
use crate::features::auth::infra::{InMemoryAuthMetadataStore, ProcessSessionAuthSecretCache};
use crate::features::auth::ports::{
    AuthEnvSecretProbe, AuthKeychainSecretStore, AuthMetadataStore, AuthSecretCache,
    AuthSecretValidator, AuthValidationFuture,
};
use crate::features::auth::usecases::{
    AuthSecretMutationUseCase, AuthSecretResolutionRequest, AuthSecretResolverUseCase,
    AuthSecretValidationRequest, AuthSecretValidationUseCase, AuthStatusRequest, AuthStatusUseCase,
    RemoveAuthSecretRequest, SetAuthSecretRequest, StdAuthSecretMutationUseCase,
    StdAuthSecretResolverUseCase, StdAuthSecretValidationUseCase, StdAuthStatusUseCase,
};
use crate::foundation::error::KernelResult;

#[test]
fn status_uses_metadata_without_keychain_secret_reads_by_default() {
    let env_probe = FakeEnvProbe::default();
    let keychain_store = FakeKeychainStore::default();
    let metadata_store = InMemoryAuthMetadataStore::new();
    metadata_store
        .save_provider_metadata(&AuthProviderMetadata {
            provider: Provider::OpenAI,
            keychain_presence: KeychainPresence::Present,
            last_updated_at: Some("2026-05-16T00:00:00Z".to_string()),
            last_validated_at: None,
            validation: AuthValidationState::Verified,
        })
        .expect("save metadata");
    let usecase = StdAuthStatusUseCase::new(&env_probe, &keychain_store, &metadata_store);

    let report = usecase
        .status(AuthStatusRequest::for_provider(
            Provider::OpenAI,
            AuthEnvLoadPolicy::ProcessOnly,
        ))
        .expect("status");

    let status = report.status_for(Provider::OpenAI).expect("openai status");
    assert!(!status.env_present);
    assert_eq!(status.keychain_presence, KeychainPresence::Present);
    assert_eq!(status.effective_source, Some(AuthSecretSource::Keychain));
    assert_eq!(status.validation, AuthValidationState::Verified);
    assert_eq!(keychain_store.presence_checks(), 0);
    assert_eq!(keychain_store.secret_reads(), 0);
}

#[test]
fn status_can_probe_keychain_presence_when_requested() {
    let env_probe = FakeEnvProbe::default();
    let keychain_store = FakeKeychainStore::default();
    let metadata_store = InMemoryAuthMetadataStore::new();
    keychain_store
        .write_keychain_secret(Provider::Anthropic, "sk-ant-test")
        .expect("write fake keychain");
    let usecase = StdAuthStatusUseCase::new(&env_probe, &keychain_store, &metadata_store);

    let report = usecase
        .status(
            AuthStatusRequest::for_provider(Provider::Anthropic, AuthEnvLoadPolicy::ProcessOnly)
                .with_keychain_probe(),
        )
        .expect("status");

    let status = report
        .status_for(Provider::Anthropic)
        .expect("anthropic status");
    assert_eq!(status.keychain_presence, KeychainPresence::Present);
    assert_eq!(status.validation, AuthValidationState::NotChecked);
    assert_eq!(keychain_store.presence_checks(), 1);
    assert_eq!(keychain_store.secret_reads(), 0);
}

#[test]
fn status_env_mode_skips_keychain_presence_probe() {
    let env_probe = FakeEnvProbe::default();
    env_probe.insert(Provider::OpenAI, "sk-from-env");
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::OpenAI, "sk-from-keychain")
        .expect("write fake keychain");
    let metadata_store = InMemoryAuthMetadataStore::new();
    metadata_store
        .save_provider_preference(&AuthProviderPreference {
            provider: Provider::OpenAI,
            source_mode: AuthSourceMode::Env,
            env_file: None,
        })
        .expect("save preference");
    let usecase = StdAuthStatusUseCase::new(&env_probe, &keychain_store, &metadata_store);

    let report = usecase
        .status(
            AuthStatusRequest::for_provider(Provider::OpenAI, AuthEnvLoadPolicy::CwdDotenvOverride)
                .with_keychain_probe(),
        )
        .expect("status");

    let status = report.status_for(Provider::OpenAI).expect("openai status");
    assert_eq!(status.preference.source_mode, AuthSourceMode::Env);
    assert!(status.env_present);
    assert_eq!(status.keychain_presence, KeychainPresence::Unknown);
    assert_eq!(status.effective_source, Some(AuthSecretSource::Env));
    assert_eq!(keychain_store.presence_checks(), 0);
    assert_eq!(keychain_store.secret_reads(), 0);
}

#[test]
fn resolver_prefers_env_and_does_not_touch_keychain() {
    let env_probe = FakeEnvProbe::default();
    env_probe.insert(Provider::OpenAI, "sk-from-env");
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::OpenAI, "sk-from-keychain")
        .expect("write fake keychain");
    let cache = ProcessSessionAuthSecretCache::new();
    let metadata_store = InMemoryAuthMetadataStore::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);

    let resolution = resolver
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            Provider::OpenAI,
            AuthEnvLoadPolicy::ProcessOnly,
        ))
        .expect("resolve secret");

    assert_eq!(resolution.source(), Some(AuthSecretSource::Env));
    assert_eq!(resolution.secret.expect("secret").secret(), "sk-from-env");
    assert!(!resolution.keychain_read_attempted);
    assert_eq!(keychain_store.secret_reads(), 0);
}

#[test]
fn resolver_env_mode_uses_process_env_only_and_skips_keychain() {
    let env_probe = FakeEnvProbe::default();
    env_probe.insert(Provider::OpenAI, "sk-from-env");
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::OpenAI, "sk-from-keychain")
        .expect("write fake keychain");
    let metadata_store = InMemoryAuthMetadataStore::new();
    metadata_store
        .save_provider_preference(&AuthProviderPreference {
            provider: Provider::OpenAI,
            source_mode: AuthSourceMode::Env,
            env_file: None,
        })
        .expect("save preference");
    let cache = ProcessSessionAuthSecretCache::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);

    let resolution = resolver
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            Provider::OpenAI,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        ))
        .expect("resolve secret");

    assert_eq!(resolution.source(), Some(AuthSecretSource::Env));
    assert_eq!(resolution.secret.expect("secret").secret(), "sk-from-env");
    assert_eq!(keychain_store.secret_reads(), 0);
}

#[test]
fn resolver_keychain_mode_skips_env() {
    let env_probe = FakeEnvProbe::default();
    env_probe.insert(Provider::OpenAI, "sk-from-env");
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::OpenAI, "sk-from-keychain")
        .expect("write fake keychain");
    let metadata_store = InMemoryAuthMetadataStore::new();
    metadata_store
        .save_provider_preference(&AuthProviderPreference {
            provider: Provider::OpenAI,
            source_mode: AuthSourceMode::Keychain,
            env_file: None,
        })
        .expect("save preference");
    let cache = ProcessSessionAuthSecretCache::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);

    let resolution = resolver
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            Provider::OpenAI,
            AuthEnvLoadPolicy::ProcessOnly,
        ))
        .expect("resolve secret");

    assert_eq!(resolution.source(), Some(AuthSecretSource::Keychain));
    assert_eq!(
        resolution.secret.expect("secret").secret(),
        "sk-from-keychain"
    );
    assert_eq!(keychain_store.secret_reads(), 1);
}

#[test]
fn resolver_none_mode_does_not_resolve_local_secret_sources() {
    let env_probe = FakeEnvProbe::default();
    env_probe.insert(Provider::OpenAI, "sk-from-env");
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::OpenAI, "sk-from-keychain")
        .expect("write fake keychain");
    let metadata_store = InMemoryAuthMetadataStore::new();
    metadata_store
        .save_provider_preference(&AuthProviderPreference {
            provider: Provider::OpenAI,
            source_mode: AuthSourceMode::None,
            env_file: None,
        })
        .expect("save preference");
    let cache = ProcessSessionAuthSecretCache::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);

    let resolution = resolver
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            Provider::OpenAI,
            AuthEnvLoadPolicy::ProcessOnly,
        ))
        .expect("resolve secret");

    assert_eq!(resolution.source(), None);
    assert_eq!(keychain_store.secret_reads(), 0);
}

#[test]
fn resolver_uses_provided_secret_before_env_and_keychain() {
    let env_probe = FakeEnvProbe::default();
    env_probe.insert(Provider::OpenAI, "sk-from-env");
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::OpenAI, "sk-from-keychain")
        .expect("write fake keychain");
    let cache = ProcessSessionAuthSecretCache::new();
    let metadata_store = InMemoryAuthMetadataStore::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);

    let resolution = resolver
        .resolve_secret(
            AuthSecretResolutionRequest::for_secret_use(
                Provider::OpenAI,
                AuthEnvLoadPolicy::ProcessOnly,
            )
            .with_prompt_secret(" sk-from-prompt \n"),
        )
        .expect("resolve provided secret");

    assert_eq!(resolution.source(), Some(AuthSecretSource::Prompt));
    assert_eq!(
        resolution.secret.expect("secret").secret(),
        "sk-from-prompt"
    );
    assert!(!resolution.keychain_read_attempted);
    assert_eq!(keychain_store.secret_reads(), 0);
    assert_eq!(
        cache
            .load_cached_secret(Provider::OpenAI)
            .expect("load cache")
            .expect("cached provided secret")
            .source,
        AuthSecretSource::Prompt
    );
}

#[test]
fn resolver_does_not_cache_request_secret_without_cache_scope() {
    let env_probe = FakeEnvProbe::default();
    let keychain_store = FakeKeychainStore::default();
    let cache = ProcessSessionAuthSecretCache::new();
    let metadata_store = InMemoryAuthMetadataStore::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);
    let mut request = AuthSecretResolutionRequest::for_secret_use(
        Provider::Anthropic,
        AuthEnvLoadPolicy::ProcessOnly,
    )
    .with_request_secret("sk-request");
    request.access_policy.cache_scope = crate::features::auth::domain::AuthSecretCacheScope::None;

    let resolution = resolver
        .resolve_secret(request)
        .expect("resolve request secret");

    assert_eq!(resolution.source(), Some(AuthSecretSource::Request));
    assert!(cache
        .load_cached_secret(Provider::Anthropic)
        .expect("load cache")
        .is_none());
}

#[test]
fn resolver_reads_keychain_once_then_uses_process_cache() {
    let env_probe = FakeEnvProbe::default();
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::HuggingFace, "hf-keychain")
        .expect("write fake keychain");
    let cache = ProcessSessionAuthSecretCache::new();
    let metadata_store = InMemoryAuthMetadataStore::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);
    let request = AuthSecretResolutionRequest::for_secret_use(
        Provider::HuggingFace,
        AuthEnvLoadPolicy::ProcessOnly,
    );

    let first = resolver
        .resolve_secret(request.clone())
        .expect("resolve first secret");
    let second = resolver
        .resolve_secret(request)
        .expect("resolve cached secret");

    assert_eq!(first.source(), Some(AuthSecretSource::Keychain));
    assert_eq!(second.source(), Some(AuthSecretSource::Keychain));
    assert!(first.keychain_read_attempted);
    assert!(!second.keychain_read_attempted);
    assert_eq!(keychain_store.secret_reads(), 1);
}

#[test]
fn mutation_sets_metadata_cache_and_removes_all_local_auth_state() {
    let keychain_store = FakeKeychainStore::default();
    let metadata_store = InMemoryAuthMetadataStore::new();
    metadata_store
        .save_provider_preference(&AuthProviderPreference {
            provider: Provider::Anthropic,
            source_mode: AuthSourceMode::Keychain,
            env_file: None,
        })
        .expect("save preference");
    let cache = ProcessSessionAuthSecretCache::new();
    let mutation = StdAuthSecretMutationUseCase::new(&keychain_store, &metadata_store, &cache);

    let set = mutation
        .set_secret(
            SetAuthSecretRequest::new(Provider::Anthropic, "sk-ant-test")
                .with_updated_at("2026-05-16T01:00:00Z"),
        )
        .expect("set secret");

    assert_eq!(set.keychain_presence, KeychainPresence::Present);
    assert_eq!(
        keychain_store
            .read_keychain_secret(
                Provider::Anthropic,
                AuthSecretAccessPolicy::resolve_for_use()
            )
            .expect("read fake keychain"),
        Some("sk-ant-test".to_string())
    );
    assert_eq!(
        cache
            .load_cached_secret(Provider::Anthropic)
            .expect("load cache")
            .expect("cached secret")
            .source,
        AuthSecretSource::Keychain
    );
    assert_eq!(
        metadata_store
            .load_provider_metadata(Provider::Anthropic)
            .expect("load metadata")
            .expect("metadata")
            .last_updated_at,
        Some("2026-05-16T01:00:00Z".to_string())
    );

    let removed = mutation
        .remove_secret(RemoveAuthSecretRequest::new(Provider::Anthropic))
        .expect("remove secret");

    assert!(removed.removed);
    assert!(metadata_store
        .load_provider_metadata(Provider::Anthropic)
        .expect("load removed metadata")
        .is_none());
    assert!(cache
        .load_cached_secret(Provider::Anthropic)
        .expect("load removed cache")
        .is_none());
    assert_eq!(
        metadata_store
            .load_provider_preference(Provider::Anthropic)
            .expect("load retained preference")
            .source_mode,
        AuthSourceMode::Keychain
    );
}

#[tokio::test]
async fn validation_resolves_secret_validates_provider_and_updates_metadata() {
    let env_probe = FakeEnvProbe::default();
    let keychain_store = FakeKeychainStore::default();
    keychain_store
        .write_keychain_secret(Provider::OpenAI, "sk-openai")
        .expect("write fake keychain");
    let metadata_store = InMemoryAuthMetadataStore::new();
    let cache = ProcessSessionAuthSecretCache::new();
    let resolver =
        StdAuthSecretResolverUseCase::new(&env_probe, &keychain_store, &metadata_store, &cache);
    let validator = FakeValidator {
        expected_secret: "sk-openai".to_string(),
        validation: AuthValidationState::Verified,
    };
    let validation = StdAuthSecretValidationUseCase::new(&resolver, &validator, &metadata_store);

    let result = validation
        .validate_secret(
            AuthSecretValidationRequest::new(AuthSecretResolutionRequest::for_secret_validation(
                Provider::OpenAI,
                AuthEnvLoadPolicy::ProcessOnly,
            ))
            .with_validated_at("2026-05-16T02:00:00Z"),
        )
        .await
        .expect("validate secret");

    assert_eq!(result.provider, Provider::OpenAI);
    assert_eq!(result.source, Some(AuthSecretSource::Keychain));
    assert_eq!(result.validation, AuthValidationState::Verified);
    let metadata = metadata_store
        .load_provider_metadata(Provider::OpenAI)
        .expect("load metadata")
        .expect("metadata");
    assert_eq!(metadata.keychain_presence, KeychainPresence::Present);
    assert_eq!(metadata.validation, AuthValidationState::Verified);
    assert_eq!(
        metadata.last_validated_at,
        Some("2026-05-16T02:00:00Z".to_string())
    );
}

#[test]
fn set_without_cache_clears_existing_process_cache() {
    let keychain_store = FakeKeychainStore::default();
    let metadata_store = InMemoryAuthMetadataStore::new();
    let cache = ProcessSessionAuthSecretCache::new();
    cache
        .save_cached_secret(AuthSecretMaterial::new(
            Provider::OpenAI,
            AuthSecretSource::Keychain,
            "old",
        ))
        .expect("seed cache");
    let mutation = StdAuthSecretMutationUseCase::new(&keychain_store, &metadata_store, &cache);

    mutation
        .set_secret(SetAuthSecretRequest::new(Provider::OpenAI, "new").without_cache())
        .expect("set secret");

    assert!(cache
        .load_cached_secret(Provider::OpenAI)
        .expect("load cache")
        .is_none());
}

#[derive(Default)]
struct FakeEnvProbe {
    secrets: Mutex<HashMap<Provider, AuthEnvSecretMaterial>>,
}

impl FakeEnvProbe {
    fn insert(&self, provider: Provider, secret: &str) {
        let mut secrets = self.secrets.lock().expect("env lock");
        secrets.insert(
            provider,
            AuthEnvSecretMaterial::new(
                provider,
                provider.env_var(),
                AuthEnvSecretOrigin::ProcessEnv,
                secret,
            ),
        );
    }
}

impl AuthEnvSecretProbe for FakeEnvProbe {
    fn probe_env_secret(
        &self,
        provider: Provider,
        _policy: AuthEnvLoadPolicy,
    ) -> KernelResult<Option<AuthEnvSecretMaterial>> {
        Ok(self
            .secrets
            .lock()
            .expect("env lock")
            .get(&provider)
            .cloned())
    }
}

#[derive(Default)]
struct FakeKeychainStore {
    secrets: Mutex<HashMap<Provider, String>>,
    presence_checks: Mutex<usize>,
    secret_reads: Mutex<usize>,
}

impl FakeKeychainStore {
    fn presence_checks(&self) -> usize {
        *self.presence_checks.lock().expect("presence lock")
    }

    fn secret_reads(&self) -> usize {
        *self.secret_reads.lock().expect("reads lock")
    }
}

impl AuthKeychainSecretStore for FakeKeychainStore {
    fn keychain_presence(&self, provider: Provider) -> KernelResult<KeychainPresence> {
        *self.presence_checks.lock().expect("presence lock") += 1;
        if self
            .secrets
            .lock()
            .expect("secret lock")
            .contains_key(&provider)
        {
            Ok(KeychainPresence::Present)
        } else {
            Ok(KeychainPresence::Absent)
        }
    }

    fn read_keychain_secret(
        &self,
        provider: Provider,
        policy: AuthSecretAccessPolicy,
    ) -> KernelResult<Option<String>> {
        assert!(policy.may_read_keychain_secret());
        *self.secret_reads.lock().expect("reads lock") += 1;
        Ok(self
            .secrets
            .lock()
            .expect("secret lock")
            .get(&provider)
            .cloned())
    }

    fn write_keychain_secret(&self, provider: Provider, secret: &str) -> KernelResult<()> {
        self.secrets
            .lock()
            .expect("secret lock")
            .insert(provider, secret.to_string());
        Ok(())
    }

    fn remove_keychain_secret(&self, provider: Provider) -> KernelResult<bool> {
        Ok(self
            .secrets
            .lock()
            .expect("secret lock")
            .remove(&provider)
            .is_some())
    }
}

struct FakeValidator {
    expected_secret: String,
    validation: AuthValidationState,
}

impl AuthSecretValidator for FakeValidator {
    fn validate<'a>(&'a self, _provider: Provider, secret: &'a str) -> AuthValidationFuture<'a> {
        Box::pin(async move {
            assert_eq!(secret, self.expected_secret);
            Ok(self.validation.clone())
        })
    }
}
