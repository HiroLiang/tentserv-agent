use crate::features::store::domain::StoreGcOutcome;
use crate::features::store::ports::StoreGarbageCollector;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayoutResolver;

use super::port::{StoreGcRequest, StoreGcResult, StoreGcUseCase};

pub struct StdStoreGcUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    garbage_collector: &'a dyn StoreGarbageCollector,
}

impl<'a> StdStoreGcUseCase<'a> {
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        garbage_collector: &'a dyn StoreGarbageCollector,
    ) -> Self {
        Self {
            layout_resolver,
            garbage_collector,
        }
    }
}

impl StoreGcUseCase for StdStoreGcUseCase<'_> {
    fn gc_stores(&self, request: StoreGcRequest) -> KernelResult<StoreGcResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let items = self.garbage_collector.collect_staging_garbage(&layout)?;
        let total_bytes = items.iter().map(|item| item.bytes).sum::<u64>();
        let removed_count = if request.apply {
            self.garbage_collector.remove_staging_garbage(&items)?;
            items.len()
        } else {
            0
        };

        Ok(StoreGcResult {
            layout,
            outcome: StoreGcOutcome {
                apply: request.apply,
                items,
                total_bytes,
                removed_count,
            },
        })
    }
}
