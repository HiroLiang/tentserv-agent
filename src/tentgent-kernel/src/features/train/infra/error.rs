use std::path::Path;

use crate::foundation::error::KernelError;

pub(super) fn train_store_error(message: impl Into<String>) -> KernelError {
    KernelError::TrainStoreUnavailable(message.into())
}

pub(super) fn train_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::TrainRuntimeUnavailable(message.into())
}

pub(super) fn path_error(action: &str, path: &Path, err: impl std::fmt::Display) -> KernelError {
    train_store_error(format!("{action} `{}` failed: {err}", path.display()))
}
