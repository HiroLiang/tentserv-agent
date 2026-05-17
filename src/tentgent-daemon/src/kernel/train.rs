use tentgent_kernel::{
    features::{
        dataset::ports::DatasetCatalogStore,
        model::ports::ModelCatalogStore,
        train::{
            infra::{
                FileLoraTrainPlanStore, FileLoraTrainRunStore, StdLoraTrainRunRefGenerator,
                StdTrainStoreLayoutInitializer, SystemTrainClock,
            },
            usecases::{StdLoraTrainPlanUseCase, StdLoraTrainRunUseCase},
        },
    },
    foundation::{layout::StdRuntimeLayoutResolver, platform::StdPlatformProbe},
};

pub struct TrainKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    platform_probe: StdPlatformProbe,
    layout_initializer: StdTrainStoreLayoutInitializer,
    plan_store: FileLoraTrainPlanStore,
    run_store: FileLoraTrainRunStore,
    clock: SystemTrainClock,
    run_refs: StdLoraTrainRunRefGenerator,
}

impl TrainKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            platform_probe: StdPlatformProbe,
            layout_initializer: StdTrainStoreLayoutInitializer,
            plan_store: FileLoraTrainPlanStore,
            run_store: FileLoraTrainRunStore::default(),
            clock: SystemTrainClock,
            run_refs: StdLoraTrainRunRefGenerator,
        }
    }

    pub fn plan_usecase<'a>(
        &'a self,
        model_catalog: &'a dyn ModelCatalogStore,
        dataset_catalog: &'a dyn DatasetCatalogStore,
    ) -> StdLoraTrainPlanUseCase<'a> {
        StdLoraTrainPlanUseCase::new(
            &self.layout_resolver,
            &self.platform_probe,
            &self.layout_initializer,
            model_catalog,
            dataset_catalog,
            &self.plan_store,
            &self.clock,
        )
    }

    pub fn run_usecase(&self) -> StdLoraTrainRunUseCase<'_> {
        StdLoraTrainRunUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.plan_store,
            &self.run_store,
            &self.clock,
            &self.run_refs,
        )
    }
}

impl Default for TrainKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}
