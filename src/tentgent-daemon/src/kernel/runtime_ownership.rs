use std::sync::Arc;

use tentgent_kernel::features::runtime_ownership::{
    RuntimeOwnershipInspectionUseCase, StdRuntimeOwnershipUseCase,
};

pub struct RuntimeOwnershipKernelComponent {
    inspection: Arc<dyn RuntimeOwnershipInspectionUseCase>,
}

impl RuntimeOwnershipKernelComponent {
    pub fn new() -> Self {
        Self::new_with_inspection(Arc::new(StdRuntimeOwnershipUseCase::default()))
    }

    pub fn new_with_inspection(inspection: Arc<dyn RuntimeOwnershipInspectionUseCase>) -> Self {
        Self { inspection }
    }

    pub fn inspection(&self) -> &dyn RuntimeOwnershipInspectionUseCase {
        self.inspection.as_ref()
    }
}

impl Default for RuntimeOwnershipKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
