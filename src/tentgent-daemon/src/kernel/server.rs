use tentgent_kernel::{
    features::{
        model::ports::ModelCatalogStore,
        server::{
            infra::{
                FileServerCatalogStore, StdServerIdentityGenerator, StdServerProcessController,
                StdServerStoreLayoutInitializer, SystemServerClock,
            },
            usecases::StdServerUseCase,
        },
    },
    foundation::layout::StdRuntimeLayoutResolver,
};

pub struct ServerKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdServerStoreLayoutInitializer,
    identity: StdServerIdentityGenerator,
    catalog: FileServerCatalogStore,
    process_controller: StdServerProcessController,
    clock: SystemServerClock,
}

impl ServerKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdServerStoreLayoutInitializer,
            identity: StdServerIdentityGenerator,
            catalog: FileServerCatalogStore::default(),
            process_controller: StdServerProcessController::default(),
            clock: SystemServerClock,
        }
    }

    pub fn usecase<'a>(&'a self, model_catalog: &'a dyn ModelCatalogStore) -> StdServerUseCase<'a> {
        StdServerUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            model_catalog,
            &self.identity,
            &self.catalog,
            &self.process_controller,
            &self.clock,
        )
    }
}

impl Default for ServerKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
