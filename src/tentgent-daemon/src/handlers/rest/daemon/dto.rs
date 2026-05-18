use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DaemonLogsResponse {
    pub logs: DaemonLogsItem,
}

#[derive(Debug, Serialize)]
pub struct DaemonLogsItem {
    pub stdout: DaemonLogMetadataItem,
    pub stderr: DaemonLogMetadataItem,
}

#[derive(Debug, Serialize)]
pub struct DaemonLogResponse {
    pub log: DaemonLogContentItem,
}

#[derive(Debug, Serialize, Clone)]
pub struct DaemonLogMetadataItem {
    pub kind: &'static str,
    pub path: String,
    pub exists: bool,
    pub total_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DaemonLogContentItem {
    pub owner: &'static str,
    pub server_ref: Option<String>,
    pub short_ref: Option<String>,
    pub kind: &'static str,
    pub path: String,
    pub exists: bool,
    pub total_bytes: u64,
    pub modified_at: Option<String>,
    pub tail_bytes: u64,
    pub truncated: bool,
    pub encoding: &'static str,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct DaemonShutdownResponse {
    pub shutdown: DaemonShutdownItem,
}

#[derive(Debug, Serialize)]
pub struct DaemonShutdownItem {
    pub accepted: bool,
    pub pid: Option<u32>,
    pub message: &'static str,
}
