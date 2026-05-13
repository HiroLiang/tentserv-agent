//! Shared kernel error and result types.

use thiserror::Error;

pub type KernelResult<T> = Result<T, KernelError>;

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("runtime profile is not ready: {profile}; {next_step}")]
    RuntimeProfileNotReady { profile: String, next_step: String },

    #[error("backend capability is not ready: {backend}; {next_step}")]
    BackendCapabilityNotReady { backend: String, next_step: String },

    #[error("capability manifest is unavailable: {0}")]
    CapabilityManifestUnavailable(String),

    #[error("runtime state is unavailable: {0}")]
    RuntimeStateUnavailable(String),

    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
}
