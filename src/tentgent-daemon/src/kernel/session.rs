use tentgent_kernel::{
    features::session::{
        infra::{
            FileSessionLockManager, FileSessionStore, StdSessionIdentityGenerator,
            SystemSessionClock,
        },
        ports::{SessionAdapterRefResolver, SessionServerRefResolver, SessionSummaryGenerator},
        usecases::StdSessionUseCase,
    },
    foundation::layout::StdRuntimeLayoutResolver,
};

pub struct SessionKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    identity: StdSessionIdentityGenerator,
    clock: SystemSessionClock,
    locks: FileSessionLockManager,
    store: FileSessionStore,
}

impl SessionKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            identity: StdSessionIdentityGenerator,
            clock: SystemSessionClock,
            locks: FileSessionLockManager::default(),
            store: FileSessionStore,
        }
    }

    pub fn usecase<'a>(
        &'a self,
        server_refs: &'a dyn SessionServerRefResolver,
        adapter_refs: &'a dyn SessionAdapterRefResolver,
        summaries: &'a dyn SessionSummaryGenerator,
    ) -> StdSessionUseCase<'a> {
        StdSessionUseCase::new(
            &self.layout_resolver,
            &self.identity,
            &self.clock,
            &self.locks,
            &self.store,
            server_refs,
            adapter_refs,
            summaries,
        )
    }
}

impl Default for SessionKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
