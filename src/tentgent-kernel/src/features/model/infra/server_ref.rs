use std::fs;

use serde::Deserialize;

use crate::features::model::domain::ModelRef;
use crate::features::model::ports::ModelServerReferenceProbe;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::error::{model_store_error, path_error};

/// Reads stored server specs to find model removal blockers.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileModelServerReferenceProbe;

impl ModelServerReferenceProbe for FileModelServerReferenceProbe {
    fn server_refs_for_model(
        &self,
        layout: &RuntimeLayout,
        model_ref: &ModelRef,
    ) -> KernelResult<Vec<String>> {
        let mut server_refs = Vec::new();
        if !layout.servers_dir.exists() {
            return Ok(server_refs);
        }

        for entry in fs::read_dir(&layout.servers_dir)
            .map_err(|err| path_error("read servers directory", &layout.servers_dir, err))?
        {
            let entry = entry.map_err(|err| {
                model_store_error(format!(
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
            let spec: StoredServerSpec = toml::from_str(&body).map_err(|err| {
                model_store_error(format!(
                    "parse server spec `{}` failed: {err}",
                    spec_path.display()
                ))
            })?;

            let Some(spec_model_ref) = spec.model_ref else {
                continue;
            };
            if spec_model_ref == model_ref.as_str()
                || model_ref.as_str().starts_with(&spec_model_ref)
            {
                server_refs.push(spec.short_ref);
            }
        }

        server_refs.sort();
        server_refs.dedup();
        Ok(server_refs)
    }
}

#[derive(Debug, Deserialize)]
struct StoredServerSpec {
    short_ref: String,
    #[serde(default)]
    model_ref: Option<String>,
}
