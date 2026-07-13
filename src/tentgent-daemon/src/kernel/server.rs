use tentgent_kernel::{
    features::{
        cluster::infra::FileClusterCatalogStore,
        model::ports::{ModelCapabilityProofStore, ModelCatalogStore},
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
    cluster_catalog: FileClusterCatalogStore,
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
            cluster_catalog: FileClusterCatalogStore,
        }
    }

    pub fn usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
        model_proofs: &'a dyn ModelCapabilityProofStore,
    ) -> StdServerUseCase<'a> {
        StdServerUseCase::new_with_cluster_catalog(
            &self.layout_resolver,
            &self.layout_initializer,
            model_catalog,
            model_proofs,
            &self.cluster_catalog,
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
