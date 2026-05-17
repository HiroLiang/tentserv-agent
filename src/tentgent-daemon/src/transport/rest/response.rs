use serde::Serialize;

pub const SERVICE_NAME: &str = "tentgent-daemon";

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}
