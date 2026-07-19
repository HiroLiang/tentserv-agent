//! Cross-process coordination for short managed-resource transitions.

mod domain;
mod ports;

pub mod infra;
pub mod usecases;

pub use domain::{
    new_operation_id, ResourceBusy, ResourceCoordinationCode, ResourceHolder, ResourceKey,
    ResourceKind, ResourceLockMode, ResourceLockRequest, ResourcePermit,
};
pub use ports::{ResourceCoordinationLease, ResourceCoordinator};
pub use usecases::{
    bounded_retry_delay, stabilize_resource_transition, ResourceTransitionAuthorization,
    StableResourceTransition,
};
