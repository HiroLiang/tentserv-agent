//! Adapter removal use case.

use std::sync::Arc;

use crate::features::adapter::domain::AdapterRemovalOutcome;
use crate::features::adapter::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterContentStore, AdapterServerReferenceProbe,
    AdapterSourceIndexStore,
};
use crate::features::resource_guard::{
    authorize_stable_resource_mutation, ResourceGuardUseCase, ResourceMutationOutcome,
    ResourceOperation, StableMutationAuthorization, StableMutationSnapshot, StdResourceGuard,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::adapter_store_layout;
use super::port::{AdapterRemoveRequest, AdapterRemoveResult, AdapterRemoveUseCase};

/// Standard adapter removal orchestration.
pub struct StdAdapterRemoveUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    adapter_catalog: &'a dyn AdapterCatalogStore,
    source_indexes: &'a dyn AdapterSourceIndexStore,
    base_indexes: &'a dyn AdapterBaseIndexStore,
    content: &'a dyn AdapterContentStore,
    guard: Arc<dyn ResourceGuardUseCase>,
}

impl<'a> StdAdapterRemoveUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        adapter_catalog: &'a dyn AdapterCatalogStore,
        source_indexes: &'a dyn AdapterSourceIndexStore,
        base_indexes: &'a dyn AdapterBaseIndexStore,
        content: &'a dyn AdapterContentStore,
        _server_refs: &'a dyn AdapterServerReferenceProbe,
    ) -> Self {
        Self::new_with_guard(
            layout_resolver,
            adapter_catalog,
            source_indexes,
            base_indexes,
            content,
            Arc::new(StdResourceGuard::default()),
        )
    }

    pub fn new_with_guard(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        adapter_catalog: &'a dyn AdapterCatalogStore,
        source_indexes: &'a dyn AdapterSourceIndexStore,
        base_indexes: &'a dyn AdapterBaseIndexStore,
        content: &'a dyn AdapterContentStore,
        guard: Arc<dyn ResourceGuardUseCase>,
    ) -> Self {
        Self {
            layout_resolver,
            adapter_catalog,
            source_indexes,
            base_indexes,
            content,
            guard,
        }
    }

    pub fn remove_adapter_guarded(
        &self,
        request: AdapterRemoveRequest,
    ) -> KernelResult<ResourceMutationOutcome<AdapterRemoveResult>> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let authorization =
            authorize_stable_resource_mutation(&layout, self.guard.as_ref(), || {
                let inspection = self
                    .adapter_catalog
                    .inspect_adapter(&store, &request.selector)?;
                Ok(StableMutationSnapshot {
                    token: AdapterRemovalDependencyToken {
                        metadata: inspection.metadata.clone(),
                        store_path: inspection.store_path.clone(),
                        source_path: inspection.source_path.clone(),
                    },
                    operation: ResourceOperation::DeleteAdapter {
                        adapter_ref: inspection.metadata.adapter_ref.to_string(),
                        base_model_ref: inspection
                            .metadata
                            .base_model_ref
                            .as_ref()
                            .map(ToString::to_string),
                        capability: inspection.metadata.target_capability,
                    },
                    state: inspection,
                })
            })?;
        let (inspection, _permit) = match authorization {
            StableMutationAuthorization::Permitted { state, permit } => (state, permit),
            StableMutationAuthorization::Rejected(rejection) => {
                return Ok(ResourceMutationOutcome::Blocked(rejection));
            }
            StableMutationAuthorization::Busy(busy) => {
                return Ok(ResourceMutationOutcome::Busy(busy));
            }
        };
        let adapter_ref = inspection.metadata.adapter_ref.clone();
        let mut removed_index_paths = self
            .source_indexes
            .remove_source_indexes(&store, &adapter_ref)?;
        removed_index_paths.extend(
            self.base_indexes
                .remove_base_model_indexes(&store, &adapter_ref)?,
        );
        removed_index_paths.sort();
        self.content.remove_adapter_content(&store, &adapter_ref)?;
        Ok(ResourceMutationOutcome::Applied(AdapterRemoveResult {
            layout,
            store,
            outcome: AdapterRemovalOutcome {
                metadata: inspection.metadata,
                store_path: inspection.store_path,
                removed_index_paths,
            },
        }))
    }
}

#[derive(PartialEq, Eq)]
struct AdapterRemovalDependencyToken {
    metadata: crate::features::adapter::domain::AdapterMetadata,
    store_path: std::path::PathBuf,
    source_path: std::path::PathBuf,
}

impl AdapterRemoveUseCase for StdAdapterRemoveUseCase<'_> {
    fn remove_adapter(&self, request: AdapterRemoveRequest) -> KernelResult<AdapterRemoveResult> {
        self.remove_adapter_guarded(request)?.into_compat_result()
    }
}
