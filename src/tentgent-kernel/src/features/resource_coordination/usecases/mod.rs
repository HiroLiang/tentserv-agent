//! Resource coordination use cases.

mod acquire;
mod stabilize;

pub use acquire::acquire_or_busy;
pub use stabilize::{
    bounded_retry_delay, stabilize_resource_transition, ResourceTransitionAuthorization,
    StableResourceTransition,
};
