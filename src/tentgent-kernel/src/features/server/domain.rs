//! Server feature domain types.

use crate::capabilities::domain::BackendKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartServerInput {
    pub server_ref: Option<String>,
    pub required_backend: Option<BackendKind>,
}
