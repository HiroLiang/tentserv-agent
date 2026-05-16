//! System keychain-backed auth secret store.

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use keyring::{Entry, Error as KeyringError};

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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn native_entry(provider: Provider) -> KernelResult<Entry> {
    Entry::new(AUTH_SERVICE, provider.keychain_account()).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to initialize system keychain entry for {}: {err}",
            provider.display_name()
        ))
    })
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn keychain_access_error(provider: Provider, err: KeyringError) -> KernelError {
    KernelError::RuntimeStateUnavailable(format!(
        "failed to access system keychain for {}: {err}",
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
