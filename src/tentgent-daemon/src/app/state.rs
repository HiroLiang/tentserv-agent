use crate::{
    bootstrap::{LoggingRuntime, RestConfig},
    runtime::{JobRegistry, MemoryCache, Scheduler},
};

use super::DaemonServices;

pub struct DaemonAppState {
    services: DaemonServices,
    logging: LoggingRuntime,
    cache: MemoryCache,
    jobs: JobRegistry,
    scheduler: Scheduler,
    rest: RestConfig,
}

impl DaemonAppState {
    pub fn new(services: DaemonServices, logging: LoggingRuntime, rest: RestConfig) -> Self {
        Self {
            services,
            logging,
            cache: MemoryCache::default(),
            jobs: JobRegistry::default(),
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
