//! Process-local auth secret cache.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::features::auth::domain::{AuthSecretMaterial, Provider};
use crate::features::auth::ports::AuthSecretCache;
use crate::foundation::error::{KernelError, KernelResult};

pub const DEFAULT_AUTH_SECRET_CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub struct ProcessSessionAuthSecretCache {
    secrets: Mutex<HashMap<Provider, CachedAuthSecret>>,
    ttl: Duration,
}

#[derive(Debug, Clone)]
struct CachedAuthSecret {
    secret: AuthSecretMaterial,
    stored_at: Instant,
}

impl ProcessSessionAuthSecretCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            secrets: Mutex::new(HashMap::new()),
            ttl,
        }
    }
}

impl Default for ProcessSessionAuthSecretCache {
    fn default() -> Self {
        Self::with_ttl(DEFAULT_AUTH_SECRET_CACHE_TTL)
    }
}

impl AuthSecretCache for ProcessSessionAuthSecretCache {
    fn load_cached_secret(&self, provider: Provider) -> KernelResult<Option<AuthSecretMaterial>> {
        let mut secrets = self.secrets.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth secret cache lock is poisoned".to_string())
        })?;
        let Some(cached) = secrets.get(&provider) else {
            return Ok(None);
        };

        if cached.stored_at.elapsed() >= self.ttl {
            secrets.remove(&provider);
            return Ok(None);
        }

        Ok(Some(cached.secret.clone()))
    }

    fn save_cached_secret(&self, secret: AuthSecretMaterial) -> KernelResult<()> {
        let mut secrets = self.secrets.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth secret cache lock is poisoned".to_string())
        })?;
        secrets.insert(
            secret.provider,
            CachedAuthSecret {
                secret,
                stored_at: Instant::now(),
            },
        );
        Ok(())
    }

    fn remove_cached_secret(&self, provider: Provider) -> KernelResult<()> {
        let mut secrets = self.secrets.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth secret cache lock is poisoned".to_string())
        })?;
        secrets.remove(&provider);
        Ok(())
    }
}
