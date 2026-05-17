//! Shared kernel error and result types.

use thiserror::Error;

pub type KernelResult<T> = Result<T, KernelError>;

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("runtime profile is not ready: {profile}; {next_step}")]
    RuntimeProfileNotReady { profile: String, next_step: String },

    #[error("backend capability is not ready: {backend}; {next_step}")]
    BackendCapabilityNotReady { backend: String, next_step: String },

    #[error("capability state is unavailable: {0}")]
    CapabilityStateUnavailable(String),

    #[error("runtime state is unavailable: {0}")]
    RuntimeStateUnavailable(String),

    #[error("model store is unavailable: {0}")]
    ModelStoreUnavailable(String),

    #[error("adapter store is unavailable: {0}")]
    AdapterStoreUnavailable(String),

    #[error("dataset store is unavailable: {0}")]
    DatasetStoreUnavailable(String),

    #[error("dataset runtime is unavailable: {0}")]
    DatasetRuntimeUnavailable(String),

    #[error("chat runtime is unavailable: {0}")]
    ChatRuntimeUnavailable(String),

    #[error("train store is unavailable: {0}")]
    TrainStoreUnavailable(String),

    #[error("train runtime is unavailable: {0}")]
    TrainRuntimeUnavailable(String),

    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
}
