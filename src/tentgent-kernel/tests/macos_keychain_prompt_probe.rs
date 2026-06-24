#![cfg(target_os = "macos")]

//! Temporary manual probes for #105 macOS Keychain prompt behavior.
//!
//! These ignored tests are intentionally not part of normal CI. Run them one
//! at a time with `-- --ignored --nocapture` and report which step triggers a
//! macOS prompt. They never print secret values.

use std::time::{SystemTime, UNIX_EPOCH};

use security_framework::base::Error as SecurityFrameworkError;
use security_framework::passwords::{
    delete_generic_password_options, generic_password, set_generic_password_options,
    AccessControlOptions, PasswordOptions,
};
use tentgent_kernel::features::auth::domain::{
    AuthEnvLoadPolicy, AuthSecretAccessPolicy, Provider, AUTH_SERVICE,
};
use tentgent_kernel::features::auth::infra::{
    FileAuthMetadataStore, InMemoryAuthMetadataStore, ProcessSessionAuthSecretCache,
    StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::ports::AuthKeychainSecretStore;
use tentgent_kernel::features::auth::usecases::{
    AuthSecretResolutionRequest, AuthSecretResolverUseCase, AuthStatusRequest, AuthStatusUseCase,
    StdAuthSecretResolverUseCase, StdAuthStatusUseCase,
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_MISSING_ENTITLEMENT: i32 = -34018;

#[test]
#[ignore = "manual live macOS Keychain prompt probe for #105"]
fn probe_01_direct_temporary_roundtrip_security_framework() {
    let account = format!("__tentgent_probe_direct_{}_{}", std::process::id(), unix_nanos());
    let secret = b"tentgent-keychain-prompt-probe";

    cleanup_account(&account);
    eprintln!("probe=direct_temporary_roundtrip service={AUTH_SERVICE} account={account}");

    let write_mode = match set_generic_password_options(
        secret,
        user_presence_protected_options(AUTH_SERVICE, &account),
    ) {
        Ok(()) => "protected-user-presence",
        Err(protected_err) => {
            eprintln!(
                "protected user-presence write failed: code={} err={protected_err}",
                protected_err.code()
            );
            match set_generic_password_options(
                secret,
                user_presence_login_options(AUTH_SERVICE, &account),
            ) {
                Ok(()) => "login-user-presence",
                Err(login_err) => {
                    eprintln!(
                        "login user-presence write failed: code={} err={login_err}",
                        login_err.code()
                    );
                    set_generic_password_options(secret, standard_options(AUTH_SERVICE, &account))
                        .unwrap_or_else(|standard_err| {
                            panic!(
                                "standard login write failed: code={} err={standard_err}",
                                standard_err.code()
                            )
                        });
                    "login-standard"
                }
            }
        }
    };
    eprintln!("write_mode={write_mode}");

    let read_location = read_direct_account(&account)
        .map(|location| location.to_string())
        .unwrap_or_else(|err| panic!("read failed: code={} err={err}", err.code()));
    eprintln!("read_location={read_location}");

    cleanup_account(&account);
    eprintln!("cleanup=done");
}

#[test]
#[ignore = "manual live macOS Keychain prompt probe for #105"]
fn probe_02_direct_provider_read_security_framework() {
    let provider = probe_provider();
    let account = provider.keychain_account();
    eprintln!(
        "probe=direct_provider_read_security_framework provider={} service={AUTH_SERVICE} account={account}",
        provider.cli_name()
    );

    match read_direct_account(account) {
        Ok(location) => eprintln!("secret_present=true read_location={location}"),
        Err(err) if item_not_found_or_unavailable(err) => {
            eprintln!("secret_present=false read_location=missing")
        }
        Err(err) => panic!("provider read failed: code={} err={err}", err.code()),
    }
}

#[test]
#[ignore = "manual live macOS Keychain prompt probe for #105"]
fn probe_03_store_provider_presence() {
    let provider = probe_provider();
    let store = SystemKeychainAuthSecretStore::new();
    eprintln!(
        "probe=store_provider_presence provider={} note=uses SystemKeychainAuthSecretStore::keychain_presence",
        provider.cli_name()
    );

    let presence = store
        .keychain_presence(provider)
        .expect("keychain presence probe");
    eprintln!("presence={presence:?}");
}

#[test]
#[ignore = "manual live macOS Keychain prompt probe for #105"]
fn probe_04_store_provider_read() {
    let provider = probe_provider();
    let store = SystemKeychainAuthSecretStore::new();
    eprintln!(
        "probe=store_provider_read provider={} note=uses SystemKeychainAuthSecretStore::read_keychain_secret",
        provider.cli_name()
    );

    let secret = store
        .read_keychain_secret(provider, AuthSecretAccessPolicy::resolve_for_use())
        .expect("keychain secret read");
    eprintln!("secret_present={}", secret.is_some());
}

#[test]
#[ignore = "manual live macOS Keychain prompt probe for #105"]
fn probe_05_auth_status_metadata_only() {
    let provider = probe_provider();
    let layout = runtime_layout();
    let metadata = FileAuthMetadataStore::from_layout(&layout);
    let env = StdAuthEnvSecretProbe;
    let keychain = SystemKeychainAuthSecretStore::new();
    let status = StdAuthStatusUseCase::new(&env, &keychain, &metadata);
    eprintln!(
        "probe=auth_status_metadata_only provider={} metadata_path={} note=must not read Keychain secret",
        provider.cli_name(),
        layout.auth_metadata_path.display()
    );

    let report = status
        .status(AuthStatusRequest::for_provider(
            provider,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        ))
        .expect("metadata-only auth status");
    let status = report.status_for(provider).expect("provider status");
    eprintln!(
        "env_present={} keychain_presence={:?} effective_source={:?} validation={}",
        status.env_present,
        status.keychain_presence,
        status.effective_source,
        status.validation.summary()
    );
}

#[test]
#[ignore = "manual live macOS Keychain prompt probe for #105"]
fn probe_06_auth_status_with_keychain_probe() {
    let provider = probe_provider();
    let metadata = InMemoryAuthMetadataStore::new();
    let env = StdAuthEnvSecretProbe;
    let keychain = SystemKeychainAuthSecretStore::new();
    let status = StdAuthStatusUseCase::new(&env, &keychain, &metadata);
    eprintln!(
        "probe=auth_status_with_keychain_probe provider={} note=explicitly calls keychain_presence",
        provider.cli_name()
    );

    let report = status
        .status(
            AuthStatusRequest::for_provider(provider, AuthEnvLoadPolicy::CwdDotenvOverride)
                .with_keychain_probe(),
        )
        .expect("auth status with keychain probe");
    let status = report.status_for(provider).expect("provider status");
    eprintln!(
        "env_present={} keychain_presence={:?} effective_source={:?} validation={}",
        status.env_present,
        status.keychain_presence,
        status.effective_source,
        status.validation.summary()
    );
}

#[test]
#[ignore = "manual live macOS Keychain prompt probe for #105"]
fn probe_07_auth_resolver_for_use() {
    let provider = probe_provider();
    let layout = runtime_layout();
    let metadata = FileAuthMetadataStore::from_layout(&layout);
    let env = StdAuthEnvSecretProbe;
    let keychain = SystemKeychainAuthSecretStore::new();
    let cache = ProcessSessionAuthSecretCache::new();
    let resolver = StdAuthSecretResolverUseCase::new(&env, &keychain, &metadata, &cache);
    eprintln!(
        "probe=auth_resolver_for_use provider={} metadata_path={} note=mimics normal provider secret resolution",
        provider.cli_name(),
        layout.auth_metadata_path.display()
    );

    let resolution = resolver
        .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
            provider,
            AuthEnvLoadPolicy::CwdDotenvOverride,
        ))
        .expect("auth resolver");
    eprintln!(
        "source={:?} keychain_read_attempted={} secret_present={}",
        resolution.source(),
        resolution.keychain_read_attempted,
        resolution.secret.is_some()
    );
}

fn probe_provider() -> Provider {
    let value = std::env::var("TENTGENT_KEYCHAIN_PROBE_PROVIDER").unwrap_or_else(|_| "hf".into());
    match value.as_str() {
        "hf" | "huggingface" => Provider::HuggingFace,
        "openai" => Provider::OpenAI,
        "anthropic" => Provider::Anthropic,
        "gemini" => Provider::Gemini,
        other => panic!(
            "invalid TENTGENT_KEYCHAIN_PROBE_PROVIDER={other}; expected hf, openai, anthropic, or gemini"
        ),
    }
}

fn runtime_layout() -> tentgent_kernel::foundation::layout::RuntimeLayout {
    StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: None,
            data_root_dir: None,
        })
        .expect("runtime layout")
}

fn read_direct_account(account: &str) -> Result<&'static str, SecurityFrameworkError> {
    match generic_password(protected_options(AUTH_SERVICE, account)) {
        Ok(_) => Ok("protected"),
        Err(err) if item_not_found_or_unavailable(err) => {
            generic_password(standard_options(AUTH_SERVICE, account)).map(|_| "login")
        }
        Err(err) => Err(err),
    }
}

fn cleanup_account(account: &str) {
    let _ = delete_generic_password_options(protected_options(AUTH_SERVICE, account));
    let _ = delete_generic_password_options(standard_options(AUTH_SERVICE, account));
}

fn protected_options(service: &str, account: &str) -> PasswordOptions {
    let mut options = standard_options(service, account);
    options.use_protected_keychain();
    options
}

fn user_presence_protected_options(service: &str, account: &str) -> PasswordOptions {
    let mut options = protected_options(service, account);
    options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
    options
}

fn user_presence_login_options(service: &str, account: &str) -> PasswordOptions {
    let mut options = standard_options(service, account);
    options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
    options
}

fn standard_options(service: &str, account: &str) -> PasswordOptions {
    PasswordOptions::new_generic_password(service, account)
}

fn item_not_found_or_unavailable(err: SecurityFrameworkError) -> bool {
    matches!(err.code(), ERR_SEC_ITEM_NOT_FOUND | ERR_SEC_MISSING_ENTITLEMENT)
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos()
}
