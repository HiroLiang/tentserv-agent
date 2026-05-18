use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::features::adapter::ports::{AdapterSourceMetadata, AdapterSourceMetadataReader};
use crate::foundation::error::KernelResult;

use super::error::{adapter_store_error, path_error};

/// Reads adapter source metadata hints from known adapter config files.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdAdapterSourceMetadataReader;

impl AdapterSourceMetadataReader for StdAdapterSourceMetadataReader {
    fn read_source_metadata(&self, source_root: &Path) -> KernelResult<AdapterSourceMetadata> {
        let path = source_root.join("adapter_config.json");
        if !path.exists() {
            return Ok(AdapterSourceMetadata::default());
        }

        let body = fs::read_to_string(&path)
            .map_err(|err| path_error("read adapter source metadata", &path, err))?;
        let config: AdapterConfig = serde_json::from_str(&body).map_err(|err| {
            adapter_store_error(format!(
                "parse adapter source metadata `{}` failed: {err}",
                path.display()
            ))
        })?;

        Ok(AdapterSourceMetadata {
            base_model_source_repo: trimmed(config.base_model_name_or_path)
                .or_else(|| trimmed(config.model)),
            base_model_source_revision: trimmed(config.revision),
            model_family: trimmed(config.model_family).or_else(|| trimmed(config.model_type)),
        })
    }
}

#[derive(Debug, Deserialize)]
struct AdapterConfig {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    base_model_name_or_path: Option<String>,
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    model_family: Option<String>,
    #[serde(default)]
    model_type: Option<String>,
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
