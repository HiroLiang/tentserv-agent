use std::path::Path;

use tentgent_kernel::foundation::error::KernelError;

use crate::transport::rest::error::RestError;

pub(super) fn auth_error(error: KernelError) -> RestError {
    RestError::kernel("provider_auth_failed", error)
}

pub(super) fn log_io_error(action: &str, path: &Path, error: std::io::Error) -> RestError {
    RestError::internal(
        "server_log_failed",
        format!("{action} `{}` failed: {error}", path.display()),
    )
}

pub(super) fn server_error(error: KernelError) -> RestError {
    match error {
        KernelError::ServerStoreUnavailable(message) => server_store_error(message),
        KernelError::ServerRuntimeUnavailable(message) => server_runtime_error(message),
        KernelError::UnsupportedTarget(message) => {
            RestError::bad_request("unsupported_target", message)
        }
        other => RestError::kernel("server_read_failed", other),
    }
}

fn server_store_error(message: String) -> RestError {
    if message.contains("already running") {
        RestError::conflict("already_running", message)
    } else if message.contains("not running") {
        RestError::conflict("not_running", message)
    } else {
        RestError::store_lookup("server_read_failed", message)
    }
}

fn server_runtime_error(message: String) -> RestError {
    if message.contains("already running") {
        RestError::conflict("already_running", message)
    } else if message.contains("not running") {
        RestError::conflict("not_running", message)
    } else {
        RestError::internal("server_runtime_failed", message)
    }
}
