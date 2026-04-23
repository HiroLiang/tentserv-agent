use keyring::{Entry, Error as KeyringError};

use crate::AUTH_SERVICE;

use super::{AuthError, Provider};

#[derive(Debug, Default, Clone)]
pub(crate) struct KeychainStore;

impl KeychainStore {
    pub(crate) fn get(&self, provider: Provider) -> Result<Option<String>, AuthError> {
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

    pub(crate) fn set(&self, provider: Provider, secret: &str) -> Result<(), AuthError> {
        let entry = self.entry(provider)?;

        entry
            .set_password(secret)
            .map_err(|err| AuthError::Keychain {
                provider,
                message: err.to_string(),
            })
    }

    pub(crate) fn remove(&self, provider: Provider) -> Result<bool, AuthError> {
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

    fn entry(&self, provider: Provider) -> Result<Entry, AuthError> {
        Entry::new(AUTH_SERVICE, provider.keychain_account()).map_err(|err| {
            AuthError::KeychainEntry {
                provider,
                message: err.to_string(),
            }
        })
    }
}
