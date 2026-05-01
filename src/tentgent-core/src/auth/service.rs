use super::{
    env, keychain::KeychainStore, validate::KeyValidator, AuthError, KeySource, KeyValidationState,
    Provider,
};

#[derive(Debug, Clone)]
pub struct KeyStatus {
    pub provider: Provider,
    pub env_present: bool,
    pub keychain_present: bool,
    pub effective_source: Option<KeySource>,
    pub validation: KeyValidationState,
}

#[derive(Debug, Clone)]
pub struct AuthManager {
    keychain: KeychainStore,
    validator: KeyValidator,
}

impl AuthManager {
    pub fn new() -> Result<Self, AuthError> {
        Ok(Self {
            keychain: KeychainStore,
            validator: KeyValidator::new()?,
        })
    }

    pub async fn key_status(&self, provider: Provider) -> Result<KeyStatus, AuthError> {
        let env_secret = env::read_provider_env(provider);
        let keychain_secret = self.keychain.get(provider)?;

        let effective = if let Some(secret) = env_secret.as_deref() {
            Some((KeySource::Env, secret))
        } else if let Some(secret) = keychain_secret.as_deref() {
            Some((KeySource::Keychain, secret))
        } else {
            None
        };

        let validation = match effective {
            Some((_, secret)) => self.validator.validate(provider, secret).await,
            None => KeyValidationState::Missing,
        };

        Ok(KeyStatus {
            provider,
            env_present: env_secret.is_some(),
            keychain_present: keychain_secret.is_some(),
            effective_source: effective.map(|(source, _)| source),
            validation,
        })
    }

    pub fn local_key_status(&self, provider: Provider) -> Result<KeyStatus, AuthError> {
        let env_secret = env::read_provider_env(provider);
        if env_secret.is_some() {
            return Ok(KeyStatus {
                provider,
                env_present: true,
                keychain_present: false,
                effective_source: Some(KeySource::Env),
                validation: KeyValidationState::NotChecked,
            });
        }
        let keychain_secret = self.keychain.get(provider)?;

        let effective_source = if keychain_secret.is_some() {
            Some(KeySource::Keychain)
        } else {
            None
        };

        Ok(KeyStatus {
            provider,
            env_present: false,
            keychain_present: keychain_secret.is_some(),
            effective_source,
            validation: KeyValidationState::NotChecked,
        })
    }

    pub async fn validate_secret(&self, provider: Provider, secret: &str) -> KeyValidationState {
        self.validator.validate(provider, secret).await
    }

    pub fn effective_secret(
        &self,
        provider: Provider,
    ) -> Result<Option<(KeySource, String)>, AuthError> {
        if let Some(secret) = env::read_provider_env(provider) {
            return Ok(Some((KeySource::Env, secret)));
        }

        Ok(self
            .keychain
            .get(provider)?
            .map(|secret| (KeySource::Keychain, secret)))
    }

    pub fn set_key(&self, provider: Provider, secret: &str) -> Result<(), AuthError> {
        self.keychain.set(provider, secret)
    }

    pub fn remove_key(&self, provider: Provider) -> Result<bool, AuthError> {
        self.keychain.remove(provider)
    }
}
