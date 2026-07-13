use std::fs;

use serde::Deserialize;

use crate::features::cluster::domain::ClusterRef;
use crate::features::cluster::ports::ClusterServerReferenceProbe;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::error::{cluster_store_error, path_error};

/// Reads stored server specs to find cluster removal blockers.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileClusterServerReferenceProbe;

impl ClusterServerReferenceProbe for FileClusterServerReferenceProbe {
    fn server_refs_for_cluster(
        &self,
        layout: &RuntimeLayout,
        cluster_ref: &ClusterRef,
    ) -> KernelResult<Vec<String>> {
        let mut refs = Vec::new();
        if !layout.servers_dir.exists() {
            return Ok(refs);
        }

        for entry in fs::read_dir(&layout.servers_dir)
            .map_err(|err| path_error("read servers directory", &layout.servers_dir, err))?
        {
            let entry = entry.map_err(|err| {
                cluster_store_error(format!(
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
            let spec: StoredClusterServerSpec = toml::from_str(&body).map_err(|err| {
                cluster_store_error(format!(
                    "parse server spec `{}` failed: {err}",
                    spec_path.display()
                ))
            })?;
            if spec.cluster_ref.as_deref() == Some(cluster_ref.as_str()) {
                refs.push(format!("server-spec {}", spec.short_ref));
            }
        }

        refs.sort();
        refs.dedup();
        Ok(refs)
    }
}

#[derive(Debug, Deserialize)]
struct StoredClusterServerSpec {
    short_ref: String,
    #[serde(default)]
    cluster_ref: Option<String>,
}
