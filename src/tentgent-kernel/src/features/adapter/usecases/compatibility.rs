//! Server-time adapter compatibility check use case.

use crate::features::adapter::domain::validate_adapter_compatibility;
use crate::features::adapter::ports::AdapterCatalogStore;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{adapter_store_error, adapter_store_layout};
use super::port::{
    AdapterCompatibilityCheckRequest, AdapterCompatibilityCheckResult,
    AdapterCompatibilityCheckUseCase,
};

/// Standard adapter compatibility validation orchestration.
pub struct StdAdapterCompatibilityCheckUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    adapter_catalog: &'a dyn AdapterCatalogStore,
}

impl<'a> StdAdapterCompatibilityCheckUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        adapter_catalog: &'a dyn AdapterCatalogStore,
    ) -> Self {
        Self {
            layout_resolver,
            adapter_catalog,
        }
    }
}

impl AdapterCompatibilityCheckUseCase for StdAdapterCompatibilityCheckUseCase<'_> {
    fn check_adapter_compatibility(
        &self,
        request: AdapterCompatibilityCheckRequest,
    ) -> KernelResult<AdapterCompatibilityCheckResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = adapter_store_layout(&layout);
        let adapter = self
            .adapter_catalog
            .inspect_adapter(&store, &request.adapter_selector)?;
        validate_adapter_compatibility(&adapter.metadata, &request.target)
            .map_err(|err| adapter_store_error(err.to_string()))?;

        Ok(AdapterCompatibilityCheckResult {
            layout,
            store,
            adapter,
        })
    }
}
