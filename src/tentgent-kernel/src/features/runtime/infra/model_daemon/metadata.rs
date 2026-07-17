use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::foundation::{
    error::{KernelError, KernelResult},
    fs::atomic_write,
    net::http_url_from_host_port,
};

use super::{ModelRuntimeCapability, ModelRuntimeDaemonEndpoint};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ModelRuntimeDaemonMetadata {
    pub host: String,
    pub port: u16,
    pub pid: u32,
    pub capability: ModelRuntimeCapability,
    pub model_ref: Option<String>,
    #[serde(default)]
    pub process_token: Option<String>,
    pub started_at: String,
}

impl ModelRuntimeDaemonMetadata {
    pub(super) fn from_endpoint(endpoint: &ModelRuntimeDaemonEndpoint) -> KernelResult<Self> {
        use time::{format_description::well_known::Rfc3339, OffsetDateTime};
        let started_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(runtime_error)?;
        Ok(Self {
            host: endpoint.host.clone(),
            port: endpoint.port,
            pid: endpoint.pid,
            capability: endpoint.capability,
            model_ref: endpoint.model_ref.clone(),
            process_token: Some(endpoint.process_token.clone()),
            started_at,
        })
    }

    pub(super) fn endpoint(&self) -> ModelRuntimeDaemonEndpoint {
        ModelRuntimeDaemonEndpoint {
            base_url: http_url_from_host_port(&self.host, self.port),
            host: self.host.clone(),
            port: self.port,
            pid: self.pid,
            process_token: self
                .process_token
                .clone()
                .unwrap_or_else(|| format!("legacy-pid-{}", self.pid)),
            capability: self.capability,
            model_ref: self.model_ref.clone(),
            policy_mismatch: None,
        }
    }
}

pub(super) fn read_metadata_if_exists(
    path: &Path,
) -> KernelResult<Option<ModelRuntimeDaemonMetadata>> {
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(path).map_err(runtime_error)?;
    toml::from_str(&body).map(Some).map_err(runtime_error)
}

pub(super) fn write_metadata(
    path: &Path,
    metadata: &ModelRuntimeDaemonMetadata,
) -> KernelResult<()> {
    let body = toml::to_string_pretty(metadata).map_err(runtime_error)?;
    atomic_write(path, body.as_bytes()).map_err(runtime_error)
}

fn runtime_error(error: impl std::fmt::Display) -> KernelError {
    KernelError::RuntimeStateUnavailable(error.to_string())
}
