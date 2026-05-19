use crate::foundation::error::KernelError;

pub(super) fn job_store_error(message: impl Into<String>) -> KernelError {
    KernelError::JobStoreUnavailable(message.into())
}
