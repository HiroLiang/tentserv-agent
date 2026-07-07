use std::fs;
use std::path::Path;

use crate::features::cluster::domain::{
    ClusterDefinition, ClusterInspection, ClusterRef, ClusterRemoveOutcome, ClusterStoreLayout,
    ClusterSummary, CLUSTER_DEFINITION_FILENAME,
};
use crate::features::cluster::ports::ClusterCatalogStore;
use crate::foundation::error::KernelResult;

use super::error::{cluster_store_error, path_error};

/// Filesystem-backed cluster definition catalog.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileClusterCatalogStore;

impl ClusterCatalogStore for FileClusterCatalogStore {
    fn list_clusters(&self, layout: &ClusterStoreLayout) -> KernelResult<Vec<ClusterSummary>> {
        let mut clusters = Vec::new();
        if !layout.clusters_dir.exists() {
            return Ok(clusters);
        }

        for definition in self.load_all_definitions(layout)? {
            let route_keys = definition.routes.keys().copied().collect();
            clusters.push(ClusterSummary {
                cluster_ref: definition.cluster_ref,
                route_keys,
            });
        }

        clusters.sort_by(|left, right| left.cluster_ref.cmp(&right.cluster_ref));
        Ok(clusters)
    }

    fn inspect_cluster(
        &self,
        layout: &ClusterStoreLayout,
        cluster_ref: &ClusterRef,
    ) -> KernelResult<ClusterInspection> {
        let definition_path = layout.cluster_definition_path(cluster_ref.as_str());
        if !definition_path.exists() {
            return Err(cluster_store_error(format!(
                "cluster `{cluster_ref}` was not found"
            )));
        }

        let definition = read_cluster_definition(&definition_path)?;
        if definition.cluster_ref != *cluster_ref {
            return Err(cluster_store_error(format!(
                "cluster definition `{}` contains cluster_ref `{}` but is stored under `{cluster_ref}`",
                definition_path.display(),
                definition.cluster_ref
            )));
        }

        Ok(inspection_for(layout, definition))
    }

    fn save_cluster(
        &self,
        layout: &ClusterStoreLayout,
        definition: &ClusterDefinition,
    ) -> KernelResult<ClusterInspection> {
        let cluster_dir = layout.cluster_dir(definition.cluster_ref.as_str());
        fs::create_dir_all(&cluster_dir)
            .map_err(|err| path_error("create cluster directory", &cluster_dir, err))?;

        let definition_path = layout.cluster_definition_path(definition.cluster_ref.as_str());
        let tmp_path = definition_path.with_extension("toml.tmp");
        let body = toml::to_string_pretty(definition).map_err(|err| {
            cluster_store_error(format!("serialize cluster definition failed: {err}"))
        })?;
        fs::write(&tmp_path, body)
            .map_err(|err| path_error("write temporary cluster definition", &tmp_path, err))?;
        fs::rename(&tmp_path, &definition_path).map_err(|err| {
            path_error("replace cluster definition", definition_path.as_path(), err)
        })?;

        self.inspect_cluster(layout, &definition.cluster_ref)
    }

    fn remove_cluster(
        &self,
        layout: &ClusterStoreLayout,
        cluster_ref: &ClusterRef,
    ) -> KernelResult<ClusterRemoveOutcome> {
        let inspection = self.inspect_cluster(layout, cluster_ref)?;
        fs::remove_dir_all(&inspection.cluster_dir)
            .map_err(|err| path_error("remove cluster directory", &inspection.cluster_dir, err))?;
        Ok(ClusterRemoveOutcome { inspection })
    }
}

impl FileClusterCatalogStore {
    fn load_all_definitions(
        &self,
        layout: &ClusterStoreLayout,
    ) -> KernelResult<Vec<ClusterDefinition>> {
        let mut definitions = Vec::new();
        if !layout.clusters_dir.exists() {
            return Ok(definitions);
        }

        for entry in fs::read_dir(&layout.clusters_dir)
            .map_err(|err| path_error("read cluster directory", &layout.clusters_dir, err))?
        {
            let entry = entry.map_err(|err| {
                cluster_store_error(format!(
                    "read entry in cluster directory `{}` failed: {err}",
                    layout.clusters_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error("read cluster entry type", entry.path().as_path(), err)
            })?;
            if !file_type.is_dir() {
                continue;
            }

            let definition_path = entry.path().join(CLUSTER_DEFINITION_FILENAME);
            if definition_path.exists() {
                definitions.push(read_cluster_definition(&definition_path)?);
            }
        }

        Ok(definitions)
    }
}

fn inspection_for(layout: &ClusterStoreLayout, definition: ClusterDefinition) -> ClusterInspection {
    let cluster_dir = layout.cluster_dir(definition.cluster_ref.as_str());
    let definition_path = layout.cluster_definition_path(definition.cluster_ref.as_str());
    ClusterInspection {
        definition,
        home_dir: layout.home_dir.clone(),
        cluster_dir,
        definition_path,
    }
}

fn read_cluster_definition(path: &Path) -> KernelResult<ClusterDefinition> {
    let body =
        fs::read_to_string(path).map_err(|err| path_error("read cluster definition", path, err))?;
    toml::from_str(&body).map_err(|err| {
        cluster_store_error(format!(
            "parse cluster definition `{}` failed: {err}",
            path.display()
        ))
    })
}
