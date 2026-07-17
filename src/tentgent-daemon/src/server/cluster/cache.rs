use std::{
    fs,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use sha2::{Digest, Sha256};
use tentgent_kernel::{
    features::cluster::{
        domain::{ClusterDefinition, ClusterRef, ClusterStoreLayout},
        infra::FileClusterCatalogStore,
        ports::ClusterCatalogStore,
    },
    foundation::layout::RuntimeLayout,
};

use super::error::ClusterServerError;

#[derive(Clone)]
pub(super) struct ClusterDefinitionCache {
    cluster_ref: ClusterRef,
    store: ClusterStoreLayout,
    state: Arc<Mutex<CachedDefinition>>,
}

#[derive(Debug, Clone)]
pub(super) struct ClusterDefinitionSnapshot {
    pub(super) definition: ClusterDefinition,
    pub(super) hash: String,
}

#[derive(Clone)]
struct CachedDefinition {
    stamp: DefinitionStamp,
    snapshot: ClusterDefinitionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DefinitionStamp {
    len: u64,
    modified: SystemTime,
}

impl ClusterDefinitionCache {
    pub(super) fn load(
        layout: &RuntimeLayout,
        cluster_ref: ClusterRef,
    ) -> Result<Self, ClusterServerError> {
        let store = ClusterStoreLayout::from_home_dir(layout.home_dir.clone());
        let cached = load_definition(&store, &cluster_ref)?;
        Ok(Self {
            cluster_ref,
            store,
            state: Arc::new(Mutex::new(cached)),
        })
    }

    pub(super) fn current(&self) -> Result<ClusterDefinitionSnapshot, ClusterServerError> {
        self.refresh(false)
    }

    pub(super) fn refresh(
        &self,
        force_hash: bool,
    ) -> Result<ClusterDefinitionSnapshot, ClusterServerError> {
        let definition_path = self
            .store
            .cluster_definition_path(self.cluster_ref.as_str());
        let stamp = definition_stamp(&definition_path)?;
        {
            let cached = self.state.lock().map_err(|_| {
                ClusterServerError::definition_reload_failed(
                    "cluster definition cache lock is poisoned".to_string(),
                )
            })?;
            if cached.stamp == stamp && !force_hash {
                return Ok(cached.snapshot.clone());
            }
        }

        let loaded = load_definition(&self.store, &self.cluster_ref)?;
        {
            let cached = self.state.lock().map_err(|_| {
                ClusterServerError::definition_reload_failed(
                    "cluster definition cache lock is poisoned".to_string(),
                )
            })?;
            validate_reload_policy(&cached.snapshot.definition, &loaded.snapshot.definition)?;
        }
        let snapshot = loaded.snapshot.clone();
        *self.state.lock().map_err(|_| {
            ClusterServerError::definition_reload_failed(
                "cluster definition cache lock is poisoned".to_string(),
            )
        })? = loaded;
        Ok(snapshot)
    }
}

fn validate_reload_policy(
    current: &ClusterDefinition,
    next: &ClusterDefinition,
) -> Result<(), ClusterServerError> {
    if current.routes == next.routes {
        return Ok(());
    }
    if current.route_update_policy
        == tentgent_kernel::features::cluster::domain::ClusterRouteUpdatePolicy::Block
    {
        let detail = if next.route_update_policy
            == tentgent_kernel::features::cluster::domain::ClusterRouteUpdatePolicy::Drain
        {
            "switch route_update_policy from block to drain in a policy-only apply before changing routes"
        } else {
            "stored route_update_policy is block; route targets cannot change while this worker is running"
        };
        return Err(ClusterServerError::definition_reload_failed(
            detail.to_string(),
        ));
    }
    Ok(())
}

fn load_definition(
    store: &ClusterStoreLayout,
    cluster_ref: &ClusterRef,
) -> Result<CachedDefinition, ClusterServerError> {
    let definition_path = store.cluster_definition_path(cluster_ref.as_str());
    let stamp = definition_stamp(&definition_path)?;
    let inspection = FileClusterCatalogStore
        .inspect_cluster(store, cluster_ref)
        .map_err(|err| ClusterServerError::definition_reload_failed(err.to_string()))?;
    let bytes = serde_json::to_vec(&inspection.definition).map_err(|err| {
        ClusterServerError::definition_reload_failed(format!(
            "serialize cluster definition for hashing failed: {err}"
        ))
    })?;
    let hash = hex::encode(Sha256::digest(bytes));
    Ok(CachedDefinition {
        stamp,
        snapshot: ClusterDefinitionSnapshot {
            definition: inspection.definition,
            hash,
        },
    })
}

fn definition_stamp(path: &std::path::Path) -> Result<DefinitionStamp, ClusterServerError> {
    let metadata = fs::metadata(path).map_err(|err| {
        ClusterServerError::definition_reload_failed(format!(
            "read cluster definition metadata `{}` failed: {err}",
            path.display()
        ))
    })?;
    let modified = metadata.modified().map_err(|err| {
        ClusterServerError::definition_reload_failed(format!(
            "read cluster definition modified time `{}` failed: {err}",
            path.display()
        ))
    })?;
    Ok(DefinitionStamp {
        len: metadata.len(),
        modified,
    })
}
