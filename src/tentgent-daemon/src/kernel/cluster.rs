use tentgent_kernel::{
    features::{
        auth::usecases::AuthStatusUseCase,
        cluster::{
            infra::{
                FileClusterCatalogStore, FileClusterServerReferenceProbe,
                StdClusterStoreLayoutInitializer,
            },
            usecases::{StdClusterReadinessUseCase, StdClusterUseCase},
        },
        model::ports::{ModelCapabilityProofStore, ModelCatalogStore},
    },
    foundation::layout::StdRuntimeLayoutResolver,
};

pub struct ClusterKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdClusterStoreLayoutInitializer,
    catalog: FileClusterCatalogStore,
    server_refs: FileClusterServerReferenceProbe,
}

impl ClusterKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdClusterStoreLayoutInitializer,
            catalog: FileClusterCatalogStore,
            server_refs: FileClusterServerReferenceProbe,
        }
    }

    pub fn usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> StdClusterUseCase<'a> {
        StdClusterUseCase::new_with_server_refs(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.catalog,
            model_catalog,
            &self.server_refs,
        )
    }

    pub fn readiness_usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
        model_proofs: &'a dyn ModelCapabilityProofStore,
        auth_status: &'a dyn AuthStatusUseCase,
    ) -> StdClusterReadinessUseCase<'a> {
        StdClusterReadinessUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            model_catalog,
            model_proofs,
            auth_status,
        )
    }
}

impl Default for ClusterKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
