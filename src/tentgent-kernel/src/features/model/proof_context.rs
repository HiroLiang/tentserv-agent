//! Explicit layout and authorization context for short proof transactions.

use crate::features::resource_coordination::ResourcePermit;
use crate::foundation::layout::RuntimeLayout;

use super::domain::{ModelMetadata, ModelStoreLayout};

/// Carries the real coordination root; model paths never imply a lock root.
pub struct ModelProofContext<'a> {
    runtime: &'a RuntimeLayout,
    store: ModelStoreLayout,
    permit: Option<&'a ResourcePermit>,
    expected_metadata: Option<&'a ModelMetadata>,
}

impl<'a> ModelProofContext<'a> {
    pub fn new(runtime: &'a RuntimeLayout) -> Self {
        Self {
            runtime,
            store: ModelStoreLayout::from_models_dir(runtime.models_dir.clone()),
            permit: None,
            expected_metadata: None,
        }
    }

    /// Reuses an existing authorization only after the store validates its scope/modes.
    pub fn with_permit(mut self, permit: &'a ResourcePermit) -> Self {
        self.permit = Some(permit);
        self
    }

    /// Rejects a record based on metadata changed since the caller observed it.
    pub fn with_expected_metadata(mut self, metadata: &'a ModelMetadata) -> Self {
        self.expected_metadata = Some(metadata);
        self
    }

    pub fn runtime(&self) -> &RuntimeLayout {
        self.runtime
    }

    pub fn store(&self) -> &ModelStoreLayout {
        &self.store
    }

    pub(crate) fn permit(&self) -> Option<&ResourcePermit> {
        self.permit
    }

    pub(crate) fn expected_metadata(&self) -> Option<&ModelMetadata> {
        self.expected_metadata
    }
}
