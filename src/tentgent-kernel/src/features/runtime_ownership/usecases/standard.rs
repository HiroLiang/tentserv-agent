use std::sync::Arc;

use crate::features::{
    resource_coordination::{infra::FileResourceCoordinator, ResourceCoordinator},
    runtime_ownership::{
        infra::{
            FileRuntimeOwnershipStore, StdOwnershipProcessProbe, StdRuntimeGenerationHealthProbe,
        },
        OwnershipProcessProbe, RuntimeGenerationHealthProbe, RuntimeOwnershipStore,
    },
    server::{infra::StdServerProcessIdentityProbe, ports::ServerProcessIdentityProbe},
};

#[derive(Clone)]
pub struct StdRuntimeOwnershipUseCase {
    pub(crate) coordinator: Arc<dyn ResourceCoordinator>,
    pub(crate) store: Arc<dyn RuntimeOwnershipStore>,
    pub(crate) process_probe: Arc<dyn OwnershipProcessProbe>,
    pub(crate) health_probe: Arc<dyn RuntimeGenerationHealthProbe>,
    pub(crate) route_owner_probe: Arc<dyn ServerProcessIdentityProbe>,
}

#[derive(Clone)]
pub struct RuntimeOwnershipDependencies {
    pub coordinator: Arc<dyn ResourceCoordinator>,
    pub store: Arc<dyn RuntimeOwnershipStore>,
    pub process_probe: Arc<dyn OwnershipProcessProbe>,
    pub health_probe: Arc<dyn RuntimeGenerationHealthProbe>,
    pub route_owner_probe: Arc<dyn ServerProcessIdentityProbe>,
}

impl Default for RuntimeOwnershipDependencies {
    fn default() -> Self {
        Self {
            coordinator: Arc::new(FileResourceCoordinator),
            store: Arc::new(FileRuntimeOwnershipStore),
            process_probe: Arc::new(StdOwnershipProcessProbe),
            health_probe: Arc::new(StdRuntimeGenerationHealthProbe),
            route_owner_probe: Arc::new(StdServerProcessIdentityProbe),
        }
    }
}

impl StdRuntimeOwnershipUseCase {
    pub fn new_with_dependencies(dependencies: RuntimeOwnershipDependencies) -> Self {
        Self {
            coordinator: dependencies.coordinator,
            store: dependencies.store,
            process_probe: dependencies.process_probe,
            health_probe: dependencies.health_probe,
            route_owner_probe: dependencies.route_owner_probe,
        }
    }
}

impl Default for StdRuntimeOwnershipUseCase {
    fn default() -> Self {
        Self::new_with_dependencies(RuntimeOwnershipDependencies::default())
    }
}
