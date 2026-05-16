use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::domain::{
    effective_source, AuthEnvLoadPolicy, AuthEnvSecretOrigin, AuthKeyStatus, AuthProviderMetadata,
    AuthProviderPreference, AuthSecretAccessPolicy, AuthSecretCacheScope, AuthSecretMaterial,
    AuthSecretReadIntent, AuthSecretSource, AuthValidationState, KeychainBiometricSupport,
    KeychainPresence, KeychainPromptPreference, Provider, AUTH_SERVICE,
};
use super::infra::{
    InMemoryAuthMetadataStore, ProcessSessionAuthSecretCache, StdAuthEnvSecretProbe,
    StdKeychainPromptPlanner, SystemKeychainAuthSecretStore,
};
use super::ports::{
    AuthEnvSecretProbe, AuthKeychainSecretStore, AuthMetadataStore, AuthSecretCache,
    KeychainPromptPlanner,
};
use crate::foundation::platform::{
    Architecture, CpuFacts, GpuFacts, OperatingSystem, PlatformFacts,
};

#[test]
fn providers_match_cli_env_and_keychain_contracts() {
    assert_eq!(AUTH_SERVICE, "com.tentserv.tentgent.auth");
    assert_eq!(
        Provider::ALL,
        [Provider::HuggingFace, Provider::OpenAI, Provider::Anthropic]
    );
    assert_eq!(Provider::HuggingFace.display_name(), "Hugging Face");
    assert_eq!(Provider::HuggingFace.cli_name(), "hf");
    assert_eq!(Provider::HuggingFace.env_var(), "HF_TOKEN");
    assert_eq!(Provider::HuggingFace.keychain_account(), "huggingface");
    assert_eq!(Provider::OpenAI.env_var(), "OPENAI_API_KEY");
    assert_eq!(Provider::Anthropic.env_var(), "ANTHROPIC_API_KEY");
}

#[test]
fn effective_source_prefers_env_over_keychain() {
    assert_eq!(
        effective_source(true, KeychainPresence::Present),
        Some(AuthSecretSource::Env)
    );
    assert_eq!(
        effective_source(false, KeychainPresence::Present),
        Some(AuthSecretSource::Keychain)
    );
    assert_eq!(effective_source(false, KeychainPresence::Absent), None);
    assert_eq!(effective_source(false, KeychainPresence::Unknown), None);
}

#[test]
fn local_status_uses_presence_without_validation() {
    let status = AuthKeyStatus::local(Provider::OpenAI, false, KeychainPresence::Present);

    assert_eq!(status.provider, Provider::OpenAI);
    assert_eq!(status.effective_source, Some(AuthSecretSource::Keychain));
    assert_eq!(status.validation, AuthValidationState::NotChecked);
    assert_eq!(status.validation.summary(), "not checked");
}

#[test]
fn validation_state_exposes_summary_and_detail() {
    let invalid = AuthValidationState::Invalid {
        reason: "provider rejected key".to_string(),
    };
    let unknown = AuthValidationState::Unknown {
        reason: "network timeout".to_string(),
    };

    assert_eq!(AuthValidationState::Missing.summary(), "missing");
    assert_eq!(AuthValidationState::Verified.summary(), "verified");
    assert_eq!(invalid.summary(), "invalid");
    assert_eq!(invalid.detail(), Some("provider rejected key"));
    assert_eq!(unknown.summary(), "unknown");
    assert_eq!(unknown.detail(), Some("network timeout"));
}

#[test]
fn status_policy_does_not_read_or_cache_keychain_secret() {
    let policy = AuthSecretAccessPolicy::non_prompting_status();

    assert_eq!(policy.read_intent, AuthSecretReadIntent::StatusOnly);
    assert_eq!(
        policy.keychain_prompt,
        KeychainPromptPreference::SystemDefault
    );
    assert_eq!(policy.cache_scope, AuthSecretCacheScope::None);
    assert!(!policy.may_read_keychain_secret());
    assert!(!policy.should_cache_for_process());
}

#[test]
fn cli_secret_policies_prefer_biometrics_and_process_session_cache() {
    let use_policy = AuthSecretAccessPolicy::cli_secret_use();
    let validate_policy = AuthSecretAccessPolicy::cli_secret_validation();

    assert_eq!(use_policy.read_intent, AuthSecretReadIntent::ResolveForUse);
    assert_eq!(
        validate_policy.read_intent,
        AuthSecretReadIntent::ResolveAndValidate
    );
    for policy in [use_policy, validate_policy] {
        assert_eq!(
            policy.keychain_prompt,
            KeychainPromptPreference::PreferBiometric
        );
        assert_eq!(policy.cache_scope, AuthSecretCacheScope::ProcessSession);
        assert!(policy.may_read_keychain_secret());
        assert!(policy.should_cache_for_process());
    }
}

#[test]
fn provider_preference_defaults_to_enabled_secret_use_policy() {
    let preference = AuthProviderPreference::default_for(Provider::Anthropic);

    assert_eq!(preference.provider, Provider::Anthropic);
    assert!(preference.enabled);
    assert_eq!(
        preference.access_policy,
        AuthSecretAccessPolicy::cli_secret_use()
    );
}

#[test]
fn process_session_cache_round_trips_secret_material() {
    let cache = ProcessSessionAuthSecretCache::new();
    let secret = AuthSecretMaterial::new(Provider::OpenAI, AuthSecretSource::Keychain, "sk-test");

    assert!(cache
        .load_cached_secret(Provider::OpenAI)
        .expect("load empty cache")
        .is_none());
    cache
        .save_cached_secret(secret.clone())
        .expect("save secret");
    assert_eq!(
        cache
            .load_cached_secret(Provider::OpenAI)
            .expect("load cached secret"),
        Some(secret)
    );
    cache
        .remove_cached_secret(Provider::OpenAI)
        .expect("remove secret");
    assert!(cache
        .load_cached_secret(Provider::OpenAI)
        .expect("load after remove")
        .is_none());
}

#[test]
fn process_session_cache_expires_secret_material_after_ttl() {
    let cache = ProcessSessionAuthSecretCache::with_ttl(Duration::ZERO);
    cache
        .save_cached_secret(AuthSecretMaterial::new(
            Provider::OpenAI,
            AuthSecretSource::Keychain,
            "sk-test",
        ))
        .expect("save secret");

    assert!(cache
        .load_cached_secret(Provider::OpenAI)
        .expect("load expired cache")
        .is_none());
}

#[test]
fn explicit_dotenv_probe_reports_secret_origin_without_using_runtime_home() {
    let path = temp_path("auth-explicit-dotenv").join(".env");
    fs::create_dir_all(path.parent().expect("dotenv parent")).expect("create dotenv parent");
    fs::write(&path, "OPENAI_API_KEY=sk-from-dotenv\nANTHROPIC_API_KEY=\n").expect("write dotenv");

    let secret = StdAuthEnvSecretProbe
        .probe_env_secret(
            Provider::OpenAI,
            AuthEnvLoadPolicy::ExplicitDotenvOverride { path: path.clone() },
        )
        .expect("probe dotenv secret")
        .expect("dotenv secret exists");

    assert_eq!(secret.provider, Provider::OpenAI);
    assert_eq!(secret.env_var, "OPENAI_API_KEY");
    assert_eq!(
        secret.origin,
        AuthEnvSecretOrigin::DotenvFile { path: path.clone() }
    );
    assert_eq!(secret.secret(), "sk-from-dotenv");
    assert_eq!(
        secret.into_secret_material(),
        AuthSecretMaterial::new(Provider::OpenAI, AuthSecretSource::Env, "sk-from-dotenv")
    );

    let empty = StdAuthEnvSecretProbe
        .probe_env_secret(
            Provider::Anthropic,
            AuthEnvLoadPolicy::ExplicitDotenvOverride { path },
        )
        .expect("probe empty dotenv secret");
    assert!(empty.is_none());
}

#[test]
fn in_memory_metadata_store_round_trips_non_secret_metadata() {
    let store = InMemoryAuthMetadataStore::new();
    let metadata = AuthProviderMetadata {
        provider: Provider::HuggingFace,
        keychain_presence: KeychainPresence::Present,
        last_updated_at: Some("2026-05-16T00:00:00Z".to_string()),
        last_validated_at: None,
        validation: AuthValidationState::NotChecked,
    };

    assert!(store
        .load_provider_metadata(Provider::HuggingFace)
        .expect("load empty metadata")
        .is_none());
    store
        .save_provider_metadata(&metadata)
        .expect("save metadata");
    assert_eq!(
        store
            .load_provider_metadata(Provider::HuggingFace)
            .expect("load metadata"),
        Some(metadata)
    );
    store
        .remove_provider_metadata(Provider::HuggingFace)
        .expect("remove metadata");
    assert!(store
        .load_provider_metadata(Provider::HuggingFace)
        .expect("load removed metadata")
        .is_none());
}

#[test]
fn generic_prompt_planner_falls_back_when_biometric_backend_is_missing() {
    let planner = StdKeychainPromptPlanner::new();
    let plan = planner
        .plan_prompt(&macos_platform(), KeychainPromptPreference::PreferBiometric)
        .expect("plan prompt");

    assert_eq!(plan.requested, KeychainPromptPreference::PreferBiometric);
    assert_eq!(plan.effective, KeychainPromptPreference::SystemDefault);
    assert!(matches!(
        plan.biometric_support,
        KeychainBiometricSupport::BackendUnsupported { .. }
    ));
    assert!(!plan.honors_biometric_preference());
}

#[test]
fn prompt_planner_honors_biometric_when_backend_is_available() {
    let planner = StdKeychainPromptPlanner::with_biometric_backend_available(true);
    let plan = planner
        .plan_prompt(&macos_platform(), KeychainPromptPreference::PreferBiometric)
        .expect("plan prompt");

    assert_eq!(plan.effective, KeychainPromptPreference::PreferBiometric);
    assert_eq!(plan.biometric_support, KeychainBiometricSupport::Supported);
    assert!(plan.honors_biometric_preference());
}

#[test]
fn prompt_planner_marks_biometric_unsupported_off_macos() {
    let planner = StdKeychainPromptPlanner::with_biometric_backend_available(true);
    let plan = planner
        .plan_prompt(&linux_platform(), KeychainPromptPreference::PreferBiometric)
        .expect("plan prompt");

    assert_eq!(plan.effective, KeychainPromptPreference::SystemDefault);
    assert!(matches!(
        plan.biometric_support,
        KeychainBiometricSupport::PlatformUnsupported { .. }
    ));
}

#[test]
fn system_keychain_store_honors_non_prompting_read_policy() {
    let store = SystemKeychainAuthSecretStore::new();

    let secret = store
        .read_keychain_secret(
            Provider::OpenAI,
            AuthSecretAccessPolicy::non_prompting_status(),
        )
        .expect("non-prompting read policy should not access keychain");

    assert!(secret.is_none());
}

#[test]
fn system_keychain_presence_smoke_is_opt_in_and_prints_observed_state() {
    let store = SystemKeychainAuthSecretStore::new();
    let provider = Provider::HuggingFace;

    if std::env::var_os("TENTGENT_RUN_KEYCHAIN_TESTS").is_none() {
        eprintln!(
            "skipping live system keychain presence smoke test for {}; set TENTGENT_RUN_KEYCHAIN_TESTS=1 to run it",
            provider.display_name()
        );
        return;
    }

    let presence = store
        .keychain_presence(provider)
        .expect("query live system keychain presence");
    eprintln!(
        "live system keychain presence for {}: {presence:?}",
        provider.display_name()
    );
}

fn macos_platform() -> PlatformFacts {
    platform(OperatingSystem::Macos, Architecture::Aarch64)
}

fn linux_platform() -> PlatformFacts {
    platform(OperatingSystem::Linux, Architecture::X86_64)
}

fn platform(os: OperatingSystem, arch: Architecture) -> PlatformFacts {
    PlatformFacts {
        os,
        arch,
        libc: None,
        cpu: CpuFacts {
            vendor: None,
            brand: None,
            features: Vec::new(),
        },
        gpu: GpuFacts {
            devices: Vec::new(),
            cuda: None,
            metal: None,
        },
    }
}

fn temp_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-kernel-auth-{label}-{}-{nanos}",
        std::process::id()
    ))
}
