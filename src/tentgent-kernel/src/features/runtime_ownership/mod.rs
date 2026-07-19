//! Durable ownership for cluster route generations and physical runtimes.

mod domain;
mod ports;

pub mod infra;
pub mod usecases;

pub use domain::*;
pub use infra::{
    FileRuntimeOwnershipStore, StdOwnershipProcessProbe, StdRuntimeGenerationHealthProbe,
};
pub use ports::*;
pub use usecases::{
    RouteClaimAcquireRequest, RouteClaimOwnershipUseCase, RouteClaimTransition,
    RuntimeGenerationAdmission, RuntimeGenerationOwnershipUseCase, RuntimeGenerationTransition,
    RuntimeOwnershipDependencies, RuntimeOwnershipInspectionUseCase,
    RuntimeOwnershipReconcileUseCase, StdRuntimeOwnershipUseCase,
};
