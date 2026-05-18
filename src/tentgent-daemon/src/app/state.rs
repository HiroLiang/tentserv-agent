use crate::{
    bootstrap::{LoggingRuntime, RestConfig},
    runtime::{JobRegistry, MemoryCache, Scheduler},
};
use tentgent_kernel::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

use super::DaemonServices;

pub struct DaemonAppState {
    services: DaemonServices,
    logging: LoggingRuntime,
    layout: RuntimeLayout,
    cache: MemoryCache,
    jobs: JobRegistry,
    scheduler: Scheduler,
    rest: RestConfig,
}

impl DaemonAppState {
    pub fn new(
        services: DaemonServices,
        logging: LoggingRuntime,
        layout: RuntimeLayout,
        rest: RestConfig,
    ) -> Self {
        let jobs = JobRegistry::from_runtime_dir(&layout.runtime_dir);
        Self {
            services,
            logging,
            layout,
            cache: MemoryCache::default(),
            jobs,
            scheduler: Scheduler::default(),
            rest,
        }
    }

    pub fn services(&self) -> &DaemonServices {
        &self.services
    }

    pub fn logging(&self) -> &LoggingRuntime {
        &self.logging
    }

    pub fn layout(&self) -> &RuntimeLayout {
        &self.layout
    }

    pub fn layout_input(&self, mode: LayoutResolveMode) -> RuntimeLayoutInput {
        RuntimeLayoutInput {
            mode,
            home_dir: Some(self.layout.home_dir.clone()),
            data_root_dir: Some(self.layout.data_root_dir.clone()),
        }
    }

    pub fn cache(&self) -> &MemoryCache {
        &self.cache
    }

    pub fn jobs(&self) -> &JobRegistry {
        &self.jobs
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    pub fn rest_config(&self) -> &RestConfig {
        &self.rest
    }
}
