use std::sync::Arc;

use crate::{
    features::runtime_ownership::{
        now_text, OwnershipOperationRecord, RuntimeOwnershipLayout, RuntimeOwnershipStore,
    },
    foundation::{error::KernelResult, layout::RuntimeLayout},
};

use super::StdRuntimeOwnershipUseCase;

pub(crate) struct OwnershipOperationGuard {
    layout: RuntimeOwnershipLayout,
    operation_id: String,
    store: Arc<dyn RuntimeOwnershipStore>,
}

impl StdRuntimeOwnershipUseCase {
    pub(crate) fn begin_operation(
        &self,
        layout: &RuntimeLayout,
        operation: impl Into<String>,
        operation_id: impl Into<String>,
        resource_keys: Vec<crate::features::resource_coordination::ResourceKey>,
    ) -> KernelResult<OwnershipOperationGuard> {
        let ownership = RuntimeOwnershipLayout::from_runtime_dir(&layout.runtime_dir);
        self.store.ensure_layout(&ownership)?;
        let operation_id = operation_id.into();
        self.store.write_operation(
            &ownership,
            &OwnershipOperationRecord {
                schema_version:
                    crate::features::runtime_ownership::RUNTIME_OWNERSHIP_SCHEMA_VERSION,
                operation_id: operation_id.clone(),
                operation: operation.into(),
                pid: std::process::id(),
                resource_keys,
                started_at: now_text(),
            },
        )?;
        Ok(OwnershipOperationGuard {
            layout: ownership,
            operation_id,
            store: Arc::clone(&self.store),
        })
    }
}

impl Drop for OwnershipOperationGuard {
    fn drop(&mut self) {
        let _ = self
            .store
            .remove_operation(&self.layout, &self.operation_id);
    }
}
