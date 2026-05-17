//! Adapter catalog read use case.

use crate::features::adapter::ports::AdapterCatalogStore;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::adapter_store_layout;
use super::port::{
    AdapterCatalogReadUseCase, AdapterInspectRequest, AdapterInspectResult, AdapterListRequest,
    AdapterListResult,
};

/// Standard adapter catalog read orchestration.
pub struct StdAdapterCatalogReadUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    catalog: &'a dyn AdapterCatalogStore,
}

impl<'a> StdAdapterCatalogReadUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        catalog: &'a dyn AdapterCatalogStore,
    ) -> Self {
        Self {
            layout_resolver,
            catalog,
        }
    }
}

impl AdapterCatalogReadUseCase for StdAdapterCatalogReadUseCase<'_> {
    fn list_adapters(&self, request: AdapterListRequest) -> KernelResult<AdapterListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let adapters = self.catalog.list_adapters(&store)?;

        Ok(AdapterListResult {
            layout,
            store,
            adapters,
        })
    }

    fn inspect_adapter(
        &self,
        request: AdapterInspectRequest,
    ) -> KernelResult<AdapterInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let adapter = self.catalog.inspect_adapter(&store, &request.selector)?;

        Ok(AdapterInspectResult {
            layout,
            store,
            adapter,
        })
    }
}
