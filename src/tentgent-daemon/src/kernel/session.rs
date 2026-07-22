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
            tentgent_kernel::features::session::usecases::SessionUseCaseDependencies {
                layout_resolver: &self.layout_resolver,
                identity: &self.identity,
                clock: &self.clock,
                locks: &self.locks,
                store: &self.store,
                server_refs,
                adapter_refs,
                summaries,
            },
        )
    }
}

impl Default for SessionKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
