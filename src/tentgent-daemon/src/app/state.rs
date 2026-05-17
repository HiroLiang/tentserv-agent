use crate::{
    bootstrap::RestConfig,
    runtime::{JobRegistry, MemoryCache, Scheduler},
};

use super::DaemonServices;

pub struct DaemonAppState {
    services: DaemonServices,
    cache: MemoryCache,
    jobs: JobRegistry,
    scheduler: Scheduler,
    rest: RestConfig,
}

impl DaemonAppState {
    pub fn new(services: DaemonServices, rest: RestConfig) -> Self {
        Self {
            services,
            cache: MemoryCache::default(),
            jobs: JobRegistry::default(),
            scheduler: Scheduler::default(),
            rest,
        }
    }

    pub fn services(&self) -> &DaemonServices {
        &self.services
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
