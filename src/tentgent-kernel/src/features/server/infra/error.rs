use std::path::Path;

use crate::foundation::error::KernelError;

pub(super) fn server_store_error(message: impl Into<String>) -> KernelError {
    KernelError::ServerStoreUnavailable(message.into())
}

pub(super) fn server_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::ServerRuntimeUnavailable(message.into())
}

pub(super) fn path_error(action: &str, path: &Path, err: impl std::fmt::Display) -> KernelError {
    server_store_error(format!("{action} `{}` failed: {err}", path.display()))
}
