use std::path::Path;

use crate::foundation::error::KernelError;

pub(super) fn session_store_error(message: impl Into<String>) -> KernelError {
    KernelError::SessionStoreUnavailable(message.into())
}

pub(super) fn path_error(action: &str, path: &Path, err: impl std::fmt::Display) -> KernelError {
    session_store_error(format!("{action} `{}` failed: {err}", path.display()))
}
