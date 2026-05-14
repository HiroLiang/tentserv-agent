//! Server application workflows.

use crate::capabilities::domain::BackendKind;
use crate::capabilities::usecases::EnsureBackendReady;
use crate::foundation::error::KernelResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartServerInput {
    pub server_ref: Option<String>,
    pub required_backend: Option<BackendKind>,
}

pub fn validate_start_server(
    ensure_backend_ready: &EnsureBackendReady<'_>,
    input: &StartServerInput,
) -> KernelResult<()> {
    if let Some(backend) = input.required_backend {
        ensure_backend_ready.run(backend)?;
    }
    Ok(())
}
