use crate::{
    bootstrap::{LoggingRuntime, RestConfig},
    runtime::JobRunner,
    runtime::{JobRegistry, MemoryCache, Scheduler},
};
use tentgent_kernel::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};
use tokio::sync::watch;

use super::DaemonServices;

pub struct DaemonAppState {
    services: DaemonServices,
    logging: LoggingRuntime,
    layout: RuntimeLayout,
    cache: MemoryCache,
    jobs: JobRegistry,
    job_runner: JobRunner,
    scheduler: Scheduler,
    rest: RestConfig,
    shutdown_tx: watch::Sender<bool>,
}

impl DaemonAppState {
    pub fn new(
        services: DaemonServices,
        logging: LoggingRuntime,
        layout: RuntimeLayout,
        rest: RestConfig,
    ) -> Self {
        let jobs = JobRegistry::from_runtime_dir(&layout.runtime_dir);
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            services,
            logging,
            layout,
            cache: MemoryCache::default(),
            jobs,
            job_runner: JobRunner::default(),
            scheduler: Scheduler::default(),
            rest,
            shutdown_tx,
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

    pub fn job_runner(&self) -> &JobRunner {
        &self.job_runner
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    pub fn rest_config(&self) -> &RestConfig {
        &self.rest
    }

    pub fn request_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    pub fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }
}
