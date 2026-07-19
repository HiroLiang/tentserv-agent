//! Shared model-runtime daemon supervisor.

mod adapters;
mod client;
mod health;
mod launcher;
mod metadata;
mod policy;
mod process;
mod supervisor;

pub use client::http_error_detail;
pub use policy::{ModelRuntimeCapability, ModelRuntimeDaemonLaunchPolicy};
pub use supervisor::{
    ModelRuntimeBinding, ModelRuntimeDaemonEndpoint, ModelRuntimeDaemonSupervisor,
    ModelRuntimeDaemonSupervisorDependencies,
};
