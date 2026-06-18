//! Auth metadata stores.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::features::auth::domain::{AuthProviderMetadata, AuthProviderPreference, Provider};
use crate::features::auth::ports::AuthMetadataStore;
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayout;

const AUTH_METADATA_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Default)]
pub struct InMemoryAuthMetadataStore {
    metadata: Mutex<HashMap<Provider, AuthProviderMetadata>>,
    preferences: Mutex<HashMap<Provider, AuthProviderPreference>>,
}

impl InMemoryAuthMetadataStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct FileAuthMetadataStore {
    path: PathBuf,
}

impl FileAuthMetadataStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_layout(layout: &RuntimeLayout) -> Self {
        Self::new(layout.auth_metadata_path.clone())
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl AuthMetadataStore for FileAuthMetadataStore {
    fn load_provider_metadata(
        &self,
        provider: Provider,
    ) -> KernelResult<Option<AuthProviderMetadata>> {
        let document = read_document(&self.path)?;
        Ok(document
            .providers
            .into_iter()
            .find(|metadata| metadata.provider == provider))
    }

    fn save_provider_metadata(&self, metadata: &AuthProviderMetadata) -> KernelResult<()> {
        let mut document = read_document(&self.path)?;
        upsert_provider_metadata(&mut document, metadata.clone());
        write_document(&self.path, &document)
    }

    fn remove_provider_metadata(&self, provider: Provider) -> KernelResult<()> {
        let mut document = read_document(&self.path)?;
        document
            .providers
            .retain(|metadata| metadata.provider != provider);
        write_document(&self.path, &document)
    }

    fn load_provider_preference(&self, provider: Provider) -> KernelResult<AuthProviderPreference> {
        let document = read_document(&self.path)?;
        Ok(document
            .preferences
            .into_iter()
            .find(|preference| preference.provider == provider)
            .unwrap_or_else(|| AuthProviderPreference::default_for(provider)))
    }

    fn save_provider_preference(&self, preference: &AuthProviderPreference) -> KernelResult<()> {
        let mut document = read_document(&self.path)?;
        upsert_provider_preference(&mut document, preference.clone());
        write_document(&self.path, &document)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthMetadataDocument {
    schema_version: u32,
    #[serde(default)]
    providers: Vec<AuthProviderMetadata>,
    #[serde(default)]
    preferences: Vec<AuthProviderPreference>,
}

impl Default for AuthMetadataDocument {
    fn default() -> Self {
        Self {
            schema_version: AUTH_METADATA_SCHEMA_VERSION,
            providers: Vec::new(),
            preferences: Vec::new(),
        }
    }
}

fn read_document(path: &Path) -> KernelResult<AuthMetadataDocument> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(AuthMetadataDocument::default());
        }
        Err(err) => {
            return Err(KernelError::RuntimeStateUnavailable(format!(
                "failed to read auth metadata `{}`: {err}",
                path.display()
            )));
        }
    };

    toml::from_str(&raw).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to parse auth metadata `{}`: {err}",
            path.display()
        ))
    })
}

fn write_document(path: &Path, document: &AuthMetadataDocument) -> KernelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            KernelError::RuntimeStateUnavailable(format!(
                "failed to create auth metadata directory `{}`: {err}",
                parent.display()
            ))
        })?;
    }

    let raw = toml::to_string_pretty(document).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!("failed to serialize auth metadata: {err}"))
    })?;

    fs::write(path, raw).map_err(|err| {
        KernelError::RuntimeStateUnavailable(format!(
            "failed to write auth metadata `{}`: {err}",
            path.display()
        ))
    })
}

fn upsert_provider_metadata(document: &mut AuthMetadataDocument, metadata: AuthProviderMetadata) {
    if let Some(existing) = document
        .providers
        .iter_mut()
        .find(|existing| existing.provider == metadata.provider)
    {
        *existing = metadata;
    } else {
        document.providers.push(metadata);
    }
}

fn upsert_provider_preference(
    document: &mut AuthMetadataDocument,
    preference: AuthProviderPreference,
) {
    if let Some(existing) = document
        .preferences
        .iter_mut()
        .find(|existing| existing.provider == preference.provider)
    {
        *existing = preference;
    } else {
        document.preferences.push(preference);
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

    fn load_provider_preference(&self, provider: Provider) -> KernelResult<AuthProviderPreference> {
        let preferences = self.preferences.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth metadata store lock is poisoned".to_string())
        })?;
        Ok(preferences
            .get(&provider)
            .cloned()
            .unwrap_or_else(|| AuthProviderPreference::default_for(provider)))
    }

    fn save_provider_preference(&self, preference: &AuthProviderPreference) -> KernelResult<()> {
        let mut preferences = self.preferences.lock().map_err(|_| {
            KernelError::RuntimeStateUnavailable("auth metadata store lock is poisoned".to_string())
        })?;
        preferences.insert(preference.provider, preference.clone());
        Ok(())
    }
}
