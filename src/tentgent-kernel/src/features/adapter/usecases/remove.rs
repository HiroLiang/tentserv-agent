//! Adapter removal use case.

use crate::features::adapter::domain::AdapterRemovalOutcome;
use crate::features::adapter::ports::{
    AdapterBaseIndexStore, AdapterCatalogStore, AdapterContentStore, AdapterServerReferenceProbe,
    AdapterSourceIndexStore,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{adapter_store_error, adapter_store_layout};
use super::port::{AdapterRemoveRequest, AdapterRemoveResult, AdapterRemoveUseCase};

/// Standard adapter removal orchestration.
pub struct StdAdapterRemoveUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    adapter_catalog: &'a dyn AdapterCatalogStore,
    source_indexes: &'a dyn AdapterSourceIndexStore,
    base_indexes: &'a dyn AdapterBaseIndexStore,
    content: &'a dyn AdapterContentStore,
    server_refs: &'a dyn AdapterServerReferenceProbe,
}

impl<'a> StdAdapterRemoveUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        adapter_catalog: &'a dyn AdapterCatalogStore,
        source_indexes: &'a dyn AdapterSourceIndexStore,
        base_indexes: &'a dyn AdapterBaseIndexStore,
        content: &'a dyn AdapterContentStore,
        server_refs: &'a dyn AdapterServerReferenceProbe,
    ) -> Self {
        Self {
            layout_resolver,
            adapter_catalog,
            source_indexes,
            base_indexes,
            content,
            server_refs,
        }
    }
}

impl AdapterRemoveUseCase for StdAdapterRemoveUseCase<'_> {
    fn remove_adapter(&self, request: AdapterRemoveRequest) -> KernelResult<AdapterRemoveResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let inspection = self
            .adapter_catalog
            .inspect_adapter(&store, &request.selector)?;
        let adapter_ref = inspection.metadata.adapter_ref.clone();
        let blockers = self
            .server_refs
            .server_refs_for_adapter(&layout, &adapter_ref)?;
        if !blockers.is_empty() {
            return Err(adapter_store_error(format!(
                "adapter `{}` is still referenced by server spec(s): {}",
                adapter_ref,
                blockers.join(", ")
            )));
        }

        let mut removed_index_paths = self
            .source_indexes
            .remove_source_indexes(&store, &adapter_ref)?;
        removed_index_paths.extend(
            self.base_indexes
                .remove_base_model_indexes(&store, &adapter_ref)?,
        );
        removed_index_paths.sort();
        self.content.remove_adapter_content(&store, &adapter_ref)?;

        Ok(AdapterRemoveResult {
            layout,
            store,
            outcome: AdapterRemovalOutcome {
                metadata: inspection.metadata,
                store_path: inspection.store_path,
                removed_index_paths,
            },
        })
    }
}
