//! Cluster feature package ports.

use crate::features::cluster::domain::{
    ClusterDefinition, ClusterInspection, ClusterRef, ClusterRemoveOutcome, ClusterStoreLayout,
    ClusterSummary,
};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

/// Ensures the cluster-store root directories exist for mutating operations.
pub trait ClusterStoreLayoutInitializer {
    /// Creates the cluster store root directory.
    fn ensure_cluster_store_layout(&self, layout: &ClusterStoreLayout) -> KernelResult<()>;
}

/// Reads and writes stored cluster definitions.
pub trait ClusterCatalogStore {
    /// Lists stored cluster summaries sorted for stable display.
    fn list_clusters(&self, layout: &ClusterStoreLayout) -> KernelResult<Vec<ClusterSummary>>;

    /// Inspects one stored cluster definition by exact cluster ref.
    fn inspect_cluster(
        &self,
        layout: &ClusterStoreLayout,
        cluster_ref: &ClusterRef,
    ) -> KernelResult<ClusterInspection>;

    /// Saves a canonical cluster definition.
    fn save_cluster(
        &self,
        layout: &ClusterStoreLayout,
        definition: &ClusterDefinition,
    ) -> KernelResult<ClusterInspection>;

    /// Removes one stored cluster definition directory.
    fn remove_cluster(
        &self,
        layout: &ClusterStoreLayout,
        cluster_ref: &ClusterRef,
    ) -> KernelResult<ClusterRemoveOutcome>;
}

/// Finds stored server specs that reference a cluster before removal.
pub trait ClusterServerReferenceProbe {
    /// Returns stable server-spec blocker labels for one cluster.
    fn server_refs_for_cluster(
        &self,
        layout: &RuntimeLayout,
        cluster_ref: &ClusterRef,
    ) -> KernelResult<Vec<String>>;
}
