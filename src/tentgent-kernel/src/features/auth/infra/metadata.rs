//! In-memory auth metadata store for tests and local orchestration.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::features::auth::domain::{AuthProviderMetadata, Provider};
use crate::features::auth::ports::AuthMetadataStore;
use crate::foundation::error::{KernelError, KernelResult};

#[derive(Debug, Default)]
pub struct InMemoryAuthMetadataStore {
    metadata: Mutex<HashMap<Provider, AuthProviderMetadata>>,
}

impl InMemoryAuthMetadataStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AuthMetadataStore for InMemoryAuthMetadataStore {
    fn load_provider_metadata(
        &self,
        provider: Provider,
    ) -> KernelResult<Option<AuthProviderMetadata>> {
        let metadata = self.metadata.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth metadata store lock is poisoned".to_string())
        })?;
        Ok(metadata.get(&provider).cloned())
    }

    fn save_provider_metadata(&self, metadata: &AuthProviderMetadata) -> KernelResult<()> {
        let mut records = self.metadata.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth metadata store lock is poisoned".to_string())
        })?;
        records.insert(metadata.provider, metadata.clone());
        Ok(())
    }

    fn remove_provider_metadata(&self, provider: Provider) -> KernelResult<()> {
        let mut metadata = self.metadata.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth metadata store lock is poisoned".to_string())
        })?;
        metadata.remove(&provider);
        Ok(())
    }
}
