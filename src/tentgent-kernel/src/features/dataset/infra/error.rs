use std::path::Path;

use crate::foundation::error::KernelError;

pub(super) fn dataset_store_error(message: impl Into<String>) -> KernelError {
    KernelError::DatasetStoreUnavailable(message.into())
}

pub(super) fn dataset_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::DatasetRuntimeUnavailable(message.into())
}

pub(super) fn path_error(action: &str, path: &Path, err: impl std::fmt::Display) -> KernelError {
    dataset_store_error(format!("{action} `{}` failed: {err}", path.display()))
}
