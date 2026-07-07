use std::fs;

use crate::features::cluster::domain::ClusterStoreLayout;
use crate::features::cluster::ports::ClusterStoreLayoutInitializer;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Filesystem layout initializer for stored cluster definitions.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdClusterStoreLayoutInitializer;

impl ClusterStoreLayoutInitializer for StdClusterStoreLayoutInitializer {
    fn ensure_cluster_store_layout(&self, layout: &ClusterStoreLayout) -> KernelResult<()> {
        fs::create_dir_all(&layout.clusters_dir)
            .map_err(|err| path_error("create cluster store directory", &layout.clusters_dir, err))
    }
}
