//! System keychain-backed auth secret store.
//!
//! This store owns native unlock behavior. macOS entries are written with
//! user-presence access control so macOS owns the available local unlock path,
//! such as Touch ID first and password fallback; other native backends use
//! their default prompt.

#[cfg(any(target_os = "linux", target_os = "windows"))]
use keyring::{Entry, Error as KeyringError};
#[cfg(target_os = "macos")]
use security_framework::base::Error as SecurityFrameworkError;
#[cfg(target_os = "macos")]
use security_framework::passwords::{
    delete_generic_password_options, generic_password, set_generic_password_options,
    AccessControlOptions, PasswordOptions,
};

use crate::features::auth::domain::{
    normalize_secret_value, AuthSecretAccessPolicy, KeychainPresence, Provider, AUTH_SERVICE,
};
use crate::features::auth::ports::AuthKeychainSecretStore;
use crate::foundation::error::{KernelError, KernelResult};

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemKeychainAuthSecretStore;

impl SystemKeychainAuthSecretStore {
    pub fn new() -> Self {
        Self
    }
}

impl AuthKeychainSecretStore for SystemKeychainAuthSecretStore {
    fn keychain_presence(&self, provider: Provider) -> KernelResult<KeychainPresence> {
        if !native_keychain_supported() {
            return Ok(KeychainPresence::Unknown);
        }

        read_native_secret(provider).map(|secret| match secret {
            Some(_) => KeychainPresence::Present,
            None => KeychainPresence::Absent,
        })
    }

    fn read_keychain_secret(
        &self,
        provider: Provider,
        policy: AuthSecretAccessPolicy,
    ) -> KernelResult<Option<String>> {
        if !policy.may_read_keychain_secret() {
            return Ok(None);
        }

        if !native_keychain_supported() {
            return Ok(None);
        }

        read_native_secret(provider)
    }

    fn write_keychain_secret(&self, provider: Provider, secret: &str) -> KernelResult<()> {
        if !native_keychain_supported() {
            return Err(unsupported_native_keychain());
        }

        write_native_secret(provider, secret)
    }

    fn remove_keychain_secret(&self, provider: Provider) -> KernelResult<bool> {
        if !native_keychain_supported() {
            return Err(unsupported_native_keychain());
        }

        remove_native_secret(provider)
    }
}

#[cfg(target_os = "macos")]
fn read_native_secret(provider: Provider) -> KernelResult<Option<String>> {
    match generic_password(macos_protected_password_options(provider)) {
        Ok(secret) => macos_secret_bytes_to_string(provider, secret).map(normalize_secret_value),
        Err(err) if macos_no_entry(err) => read_macos_legacy_secret(provider),
        Err(err) => Err(macos_keychain_access_error(provider, err)),
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn read_native_secret(provider: Provider) -> KernelResult<Option<String>> {
    let entry = native_entry(provider)?;

    match entry.get_password() {
        Ok(secret) => Ok(normalize_secret_value(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(keychain_access_error(provider, err)),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn read_native_secret(provider: Provider) -> KernelResult<Option<String>> {
    let _ = provider;
    Ok(None)
}

#[cfg(target_os = "macos")]
fn write_native_secret(provider: Provider, secret: &str) -> KernelResult<()> {
    remove_macos_secret_if_present(provider)?;

    set_macos_secret_for(AUTH_SERVICE, provider.keychain_account(), secret.as_bytes()).map_err(
        |(protected_err, login_err, standard_err)| {
            macos_keychain_write_error(provider, protected_err, login_err, standard_err)
        },
    )?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn write_native_secret(provider: Provider, secret: &str) -> KernelResult<()> {
    let entry = native_entry(provider)?;
    entry
        .set_password(secret)
        .map_err(|err| keychain_access_error(provider, err))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn write_native_secret(provider: Provider, secret: &str) -> KernelResult<()> {
    let _ = (provider, secret);
    Err(unsupported_native_keychain())
}

#[cfg(target_os = "macos")]
fn remove_native_secret(provider: Provider) -> KernelResult<bool> {
    remove_macos_secret_if_present(provider)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn remove_native_secret(provider: Provider) -> KernelResult<bool> {
    let entry = native_entry(provider)?;

    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(KeyringError::NoEntry) => Ok(false),
        Err(err) => Err(keychain_access_error(provider, err)),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn remove_native_secret(provider: Provider) -> KernelResult<bool> {
    let _ = provider;
    Err(unsupported_native_keychain())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn native_entry(provider: Provider) -> KernelResult<Entry> {
    Entry::new(AUTH_SERVICE, provider.keychain_account()).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to initialize system keychain entry for {}: {err}",
            provider.display_name()
        ))
    })
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn keychain_access_error(provider: Provider, err: KeyringError) -> KernelError {
    KernelError::RuntimeStateUnavailable(format!(
        "failed to access system keychain for {}: {err}",
        provider.display_name()
    ))
}

#[cfg(target_os = "macos")]
fn macos_password_options(provider: Provider) -> PasswordOptions {
    macos_password_options_for(AUTH_SERVICE, provider.keychain_account())
}

#[cfg(target_os = "macos")]
fn macos_password_options_for(service: &str, account: &str) -> PasswordOptions {
    PasswordOptions::new_generic_password(service, account)
}

#[cfg(target_os = "macos")]
fn macos_protected_password_options(provider: Provider) -> PasswordOptions {
    macos_protected_password_options_for(AUTH_SERVICE, provider.keychain_account())
}

#[cfg(target_os = "macos")]
fn macos_protected_password_options_for(service: &str, account: &str) -> PasswordOptions {
    let mut options = macos_password_options_for(service, account);
    options.use_protected_keychain();
    options
}

#[cfg(target_os = "macos")]
fn macos_user_presence_password_options_for(service: &str, account: &str) -> PasswordOptions {
    let mut options = macos_protected_password_options_for(service, account);
    options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
    options
}

#[cfg(target_os = "macos")]
fn macos_login_user_presence_password_options_for(service: &str, account: &str) -> PasswordOptions {
    let mut options = macos_password_options_for(service, account);
    options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
    options
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacosKeychainWriteMode {
    UserPresence,
    Standard,
}

#[cfg(target_os = "macos")]
fn set_macos_secret_for(
    service: &str,
    account: &str,
    secret: &[u8],
) -> Result<
    MacosKeychainWriteMode,
    (
        SecurityFrameworkError,
        SecurityFrameworkError,
        SecurityFrameworkError,
    ),
> {
    match set_generic_password_options(
        secret,
        macos_user_presence_password_options_for(service, account),
    ) {
        Ok(()) => Ok(MacosKeychainWriteMode::UserPresence),
        Err(protected_err) => match set_generic_password_options(
            secret,
            macos_login_user_presence_password_options_for(service, account),
        ) {
            Ok(()) => Ok(MacosKeychainWriteMode::UserPresence),
            Err(login_err) => {
                match set_generic_password_options(
                    secret,
                    macos_password_options_for(service, account),
                ) {
                    Ok(()) => Ok(MacosKeychainWriteMode::Standard),
                    Err(standard_err) => Err((protected_err, login_err, standard_err)),
                }
            }
        },
    }
}

#[cfg(target_os = "macos")]
fn remove_macos_secret_if_present(provider: Provider) -> KernelResult<bool> {
    let protected_removed =
        remove_macos_secret_with_options(provider, macos_protected_password_options(provider))?;
    let legacy_removed =
        remove_macos_secret_with_options(provider, macos_password_options(provider))?;

    Ok(protected_removed || legacy_removed)
}

#[cfg(target_os = "macos")]
fn read_macos_legacy_secret(provider: Provider) -> KernelResult<Option<String>> {
    match generic_password(macos_password_options(provider)) {
        Ok(secret) => macos_secret_bytes_to_string(provider, secret).map(normalize_secret_value),
        Err(err) if macos_no_entry(err) => Ok(None),
        Err(err) => Err(macos_keychain_access_error(provider, err)),
    }
}

#[cfg(target_os = "macos")]
fn remove_macos_secret_with_options(
    provider: Provider,
    options: PasswordOptions,
) -> KernelResult<bool> {
    match delete_generic_password_options(options) {
        Ok(()) => Ok(true),
        Err(err) if macos_no_entry(err) => Ok(false),
        Err(err) => Err(macos_keychain_access_error(provider, err)),
    }
}

#[cfg(target_os = "macos")]
fn macos_secret_bytes_to_string(provider: Provider, secret: Vec<u8>) -> KernelResult<String> {
    String::from_utf8(secret).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "system keychain returned a non-UTF-8 secret for {}: {err}",
            provider.display_name()
        ))
    })
}

#[cfg(target_os = "macos")]
fn macos_no_entry(err: SecurityFrameworkError) -> bool {
    err.code() == -25300
}

#[cfg(target_os = "macos")]
fn macos_keychain_access_error(provider: Provider, err: SecurityFrameworkError) -> KernelError {
    KernelError::RuntimeStateUnavailable(format!(
        "failed to access system keychain for {}: {err}",
        provider.display_name()
    ))
}

#[cfg(target_os = "macos")]
fn macos_keychain_write_error(
    provider: Provider,
    protected_err: SecurityFrameworkError,
    login_err: SecurityFrameworkError,
    standard_err: SecurityFrameworkError,
) -> KernelError {
    KernelError::RuntimeStateUnavailable(format!(
        "failed to write system keychain secret for {} with user-presence access control in the Data Protection Keychain ({protected_err}) or login keychain ({login_err}), and also failed to write a standard login keychain fallback ({standard_err})",
        provider.display_name()
    ))
}

fn native_keychain_supported() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    ))
}

fn unsupported_native_keychain() -> KernelError {
    KernelError::UnsupportedTarget(
        "native system keychain storage is supported only on macOS, Windows, and Linux".to_string(),
    )
}

#[cfg(all(test, target_os = "macos"))]
mod macos_live_tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use security_framework::passwords::{delete_generic_password_options, generic_password};

    use super::*;

    #[test]
    fn protected_keychain_roundtrip_smoke_is_opt_in_and_prints_observed_state() {
        if std::env::var_os("TENTGENT_RUN_KEYCHAIN_TOUCH_ID_TESTS").is_none() {
            eprintln!(
                "skipping live macOS protected keychain roundtrip test; set TENTGENT_RUN_KEYCHAIN_TOUCH_ID_TESTS=1 to run it"
            );
            return;
        }

        let account = format!("touch-id-smoke-{}-{}", std::process::id(), unix_nanos());
        let secret = "tentgent-keychain-touch-id-smoke";

        let _ = delete_generic_password_options(macos_protected_password_options_for(
            AUTH_SERVICE,
            &account,
        ));
        let _ = delete_generic_password_options(macos_password_options_for(AUTH_SERVICE, &account));

        let write_mode = set_macos_secret_for(AUTH_SERVICE, &account, secret.as_bytes())
            .map_err(|(protected_err, login_err, standard_err)| {
                format!(
                    "Data Protection Keychain write failed with {protected_err}; login keychain write failed with {login_err}; standard login keychain write failed with {standard_err}"
                )
            })
            .expect("write macOS keychain item");
        eprintln!("live macOS keychain write: ok account={account} mode={write_mode:?}");

        let (location, bytes) =
            match generic_password(macos_protected_password_options_for(AUTH_SERVICE, &account)) {
                Ok(bytes) => ("data-protection", bytes),
                Err(err) if macos_no_entry(err) => (
                    "login",
                    generic_password(macos_password_options_for(AUTH_SERVICE, &account))
                        .expect("read macOS login keychain item"),
                ),
                Err(err) => panic!("read macOS keychain item: {err}"),
            };
        let observed = String::from_utf8(bytes).expect("keychain test secret should be UTF-8");
        assert_eq!(observed, secret);
        eprintln!("live macOS keychain read: ok account={account} location={location}");

        let protected_removed = delete_generic_password_options(
            macos_protected_password_options_for(AUTH_SERVICE, &account),
        )
        .is_ok();
        let login_removed =
            delete_generic_password_options(macos_password_options_for(AUTH_SERVICE, &account))
                .is_ok();
        assert!(
            protected_removed || login_removed,
            "remove macOS keychain item"
        );
        eprintln!("live macOS keychain remove: ok account={account}");
    }

    fn unix_nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    }
}
