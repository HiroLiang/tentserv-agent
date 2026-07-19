//! Structured resource mutation policy and blockers.

mod domain;
mod ports;
mod registry;
mod stabilization;

pub mod probes;
pub mod validators;

pub use domain::*;
pub use ports::ResourceGuardUseCase;
pub use registry::{ResourceGuardDependencies, StdResourceGuard};
pub use stabilization::{
    authorize_stable_resource_mutation, StableMutationAuthorization, StableMutationSnapshot,
};
