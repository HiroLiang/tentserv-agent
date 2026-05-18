use std::fs;

use serde::Deserialize;

use crate::features::adapter::domain::AdapterRef;
use crate::features::adapter::ports::AdapterServerReferenceProbe;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::error::{adapter_store_error, path_error};

/// Reads stored server specs to find adapter removal blockers.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileAdapterServerReferenceProbe;

impl AdapterServerReferenceProbe for FileAdapterServerReferenceProbe {
    fn server_refs_for_adapter(
        &self,
        layout: &RuntimeLayout,
        adapter_ref: &AdapterRef,
    ) -> KernelResult<Vec<String>> {
        let mut server_refs = Vec::new();
        if !layout.servers_dir.exists() {
            return Ok(server_refs);
        }

        for entry in fs::read_dir(&layout.servers_dir)
            .map_err(|err| path_error("read servers directory", &layout.servers_dir, err))?
        {
            let entry = entry.map_err(|err| {
                adapter_store_error(format!(
                    "read entry in servers directory `{}` failed: {err}",
                    layout.servers_dir.display()
                ))
            })?;
            let file_type = entry
                .file_type()
                .map_err(|err| path_error("read server entry type", entry.path().as_path(), err))?;
            if !file_type.is_dir() {
                continue;
            }

            let spec_path = entry.path().join("server.toml");
            if !spec_path.exists() {
                continue;
            }

            let body = fs::read_to_string(&spec_path)
                .map_err(|err| path_error("read server spec", &spec_path, err))?;
            let spec: StoredServerAdapterRefs = toml::from_str(&body).map_err(|err| {
                adapter_store_error(format!(
                    "parse server spec `{}` failed: {err}",
                    spec_path.display()
                ))
            })?;

            if spec.references_adapter(adapter_ref) {
                server_refs.push(
                    spec.short_ref
                        .filter(|short_ref| !short_ref.is_empty())
                        .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned()),
                );
            }
        }

        server_refs.sort();
        server_refs.dedup();
        Ok(server_refs)
    }
}

#[derive(Debug, Deserialize)]
struct StoredServerAdapterRefs {
    #[serde(default)]
    short_ref: Option<String>,
    #[serde(default)]
    adapter_ref: Option<String>,
    #[serde(default)]
    default_adapter_ref: Option<String>,
    #[serde(default)]
    allowed_adapters: Vec<String>,
    #[serde(default)]
    adapter_refs: Vec<String>,
}

impl StoredServerAdapterRefs {
    fn references_adapter(&self, adapter_ref: &AdapterRef) -> bool {
        self.adapter_ref
            .as_deref()
            .is_some_and(|reference| adapter_ref_matches(adapter_ref, reference))
            || self
                .default_adapter_ref
                .as_deref()
                .is_some_and(|reference| adapter_ref_matches(adapter_ref, reference))
            || self
                .allowed_adapters
                .iter()
                .any(|reference| adapter_ref_matches(adapter_ref, reference))
            || self
                .adapter_refs
                .iter()
                .any(|reference| adapter_ref_matches(adapter_ref, reference))
    }
}

fn adapter_ref_matches(adapter_ref: &AdapterRef, reference: &str) -> bool {
    adapter_ref.as_str() == reference || adapter_ref.as_str().starts_with(reference)
}
