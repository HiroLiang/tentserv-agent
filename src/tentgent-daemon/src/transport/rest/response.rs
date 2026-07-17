use serde::Serialize;
use tentgent_kernel::features::resource_guard::ResourceBlocker;

pub const SERVICE_NAME: &str = "tentgent-daemon";

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<ResourceBlocker>,
}
