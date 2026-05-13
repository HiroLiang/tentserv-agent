//! Server application workflows.

use crate::capabilities::domain::BackendKind;
use crate::capabilities::service::CapabilityRead;
use crate::foundation::error::KernelResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartServerInput {
    pub server_ref: Option<String>,
    pub required_backend: Option<BackendKind>,
}

pub fn validate_start_server(
    capabilities: &impl CapabilityRead,
    input: &StartServerInput,
) -> KernelResult<()> {
    if let Some(backend) = input.required_backend {
        capabilities.ensure_backend_ready(backend)?;
    }
    Ok(())
}
