use std::path::Path;

use crate::foundation::error::KernelError;

pub(super) fn adapter_store_error(message: impl Into<String>) -> KernelError {
    KernelError::AdapterStoreUnavailable(message.into())
}

pub(super) fn path_error(action: &str, path: &Path, err: impl std::fmt::Display) -> KernelError {
    adapter_store_error(format!("{action} `{}` failed: {err}", path.display()))
}
