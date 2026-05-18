use std::path::Path;

use crate::foundation::error::KernelError;

pub(super) fn daemon_store_error(message: impl Into<String>) -> KernelError {
    KernelError::DaemonStoreUnavailable(message.into())
}

pub(super) fn daemon_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::DaemonRuntimeUnavailable(message.into())
}

pub(super) fn path_error(action: &str, path: &Path, err: impl std::fmt::Display) -> KernelError {
    daemon_store_error(format!("{action} `{}` failed: {err}", path.display()))
}
