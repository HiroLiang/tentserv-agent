//! Managed store maintenance ports.

use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::domain::StoreStagingGarbageItem;

pub trait StoreGarbageCollector {
    fn collect_staging_garbage(
        &self,
        layout: &RuntimeLayout,
    ) -> KernelResult<Vec<StoreStagingGarbageItem>>;

    fn remove_staging_garbage(&self, items: &[StoreStagingGarbageItem]) -> KernelResult<()>;
}
