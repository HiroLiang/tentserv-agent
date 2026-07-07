use tentgent_kernel::{
    features::{
        cluster::{
            infra::{FileClusterCatalogStore, StdClusterStoreLayoutInitializer},
            usecases::StdClusterUseCase,
        },
        model::ports::ModelCatalogStore,
    },
    foundation::layout::StdRuntimeLayoutResolver,
};

pub struct ClusterKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdClusterStoreLayoutInitializer,
    catalog: FileClusterCatalogStore,
}

impl ClusterKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdClusterStoreLayoutInitializer,
            catalog: FileClusterCatalogStore,
        }
    }

    pub fn usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
    ) -> StdClusterUseCase<'a> {
        StdClusterUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.catalog,
            model_catalog,
        )
    }
}

impl Default for ClusterKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
