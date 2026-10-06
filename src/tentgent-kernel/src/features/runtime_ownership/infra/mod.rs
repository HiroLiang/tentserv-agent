//! File and process adapters for runtime ownership.

mod health_probe;
mod process_identity;
mod store;

pub use health_probe::StdRuntimeGenerationHealthProbe;
pub use process_identity::StdOwnershipProcessProbe;
pub use store::FileRuntimeOwnershipStore;

#[cfg(test)]
mod process_identity_tests;
#[cfg(test)]
mod tests;
