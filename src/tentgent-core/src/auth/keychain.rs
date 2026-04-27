#[cfg(any(target_os = "macos", target_os = "windows"))]
use keyring::{Entry, Error as KeyringError};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::AUTH_SERVICE;

use super::{AuthError, Provider};

#[derive(Debug, Default, Clone)]
pub(crate) struct KeychainStore;

impl KeychainStore {
    pub(crate) fn get(&self, provider: Provider) -> Result<Option<String>, AuthError> {
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = provider;
            return Ok(None);
        }

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let entry = self.entry(provider)?;

            match entry.get_password() {
                Ok(secret) => {
                    let trimmed = secret.trim().to_owned();
                    if trimmed.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(trimmed))
                    }
                }
                Err(KeyringError::NoEntry) => Ok(None),
                Err(err) => Err(AuthError::Keychain {
                    provider,
                    message: err.to_string(),
                }),
            }
        }
    }

    pub(crate) fn set(&self, provider: Provider, secret: &str) -> Result<(), AuthError> {
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = secret;
            return Err(AuthError::Keychain {
                provider,
                message: "native keychain storage is not supported on this platform yet"
                    .to_string(),
            });
        }

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let entry = self.entry(provider)?;

            entry
                .set_password(secret)
                .map_err(|err| AuthError::Keychain {
                    provider,
                    message: err.to_string(),
                })
        }
    }

    pub(crate) fn remove(&self, provider: Provider) -> Result<bool, AuthError> {
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            return Err(AuthError::Keychain {
                provider,
                message: "native keychain storage is not supported on this platform yet"
                    .to_string(),
            });
        }

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let entry = self.entry(provider)?;

            match entry.delete_credential() {
                Ok(()) => Ok(true),
                Err(KeyringError::NoEntry) => Ok(false),
                Err(err) => Err(AuthError::Keychain {
                    provider,
                    message: err.to_string(),
                }),
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn entry(&self, provider: Provider) -> Result<Entry, AuthError> {
        Entry::new(AUTH_SERVICE, provider.keychain_account()).map_err(|err| {
            AuthError::KeychainEntry {
                provider,
                message: err.to_string(),
            }
        })
    }
}
