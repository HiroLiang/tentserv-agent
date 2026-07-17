use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use tentgent_kernel::{
    features::{
        cluster::domain::{ClusterRef, ClusterRouteKey},
        runtime_ownership::{
            new_process_token, ProcessInstanceIdentity, RouteClaimAcquireRequest,
            RouteClaimOwnershipUseCase, RouteClaimTransition, RouteGenerationClaim,
            RuntimeExecutionIdentity, StdRuntimeOwnershipUseCase,
        },
    },
    foundation::layout::RuntimeLayout,
};

use super::error::ClusterServerError;

#[derive(Clone)]
pub(super) struct RouteGenerationManager {
    layout: RuntimeLayout,
    server_ref: String,
    cluster_ref: ClusterRef,
    process: ProcessInstanceIdentity,
    ownership: Arc<dyn RouteClaimOwnershipUseCase>,
    state: Arc<Mutex<RouteGenerationState>>,
}

struct RouteGenerationState {
    accepting: bool,
    preserve_claims: bool,
    entries: BTreeMap<String, RouteGenerationEntry>,
}

struct RouteGenerationEntry {
    claim: RouteGenerationClaim,
    active_requests: usize,
    retiring: bool,
}

#[derive(Clone)]
pub(super) struct RouteRequestLease {
    lease: Arc<RouteRequestLeaseInner>,
}

struct RouteRequestLeaseInner {
    manager: RouteGenerationManager,
    owner_id: String,
}

impl RouteGenerationManager {
    pub(super) fn new(layout: RuntimeLayout, server_ref: String, cluster_ref: ClusterRef) -> Self {
        Self::new_with_ownership(
            layout,
            server_ref,
            cluster_ref,
            Arc::new(StdRuntimeOwnershipUseCase::default()),
        )
    }

    pub(super) fn new_with_ownership(
        layout: RuntimeLayout,
        server_ref: String,
        cluster_ref: ClusterRef,
        ownership: Arc<dyn RouteClaimOwnershipUseCase>,
    ) -> Self {
        Self {
            layout,
            server_ref,
            cluster_ref,
            process: ProcessInstanceIdentity::current(
                tentgent_kernel::features::server::infra::server_process_token_from_env()
                    .unwrap_or_else(new_process_token),
            ),
            ownership,
            state: Arc::new(Mutex::new(RouteGenerationState {
                accepting: true,
                preserve_claims: false,
                entries: BTreeMap::new(),
            })),
        }
    }

    pub(super) fn acquire(
        &self,
        route: ClusterRouteKey,
        definition_hash: &str,
        target: RuntimeExecutionIdentity,
    ) -> Result<RouteRequestLease, ClusterServerError> {
        let claim = RouteGenerationClaim::new(
            self.server_ref.clone(),
            self.cluster_ref.clone(),
            route,
            definition_hash,
            target,
            self.process.clone(),
        );
        {
            let mut state = self.lock_state()?;
            if !state.accepting {
                return Err(ClusterServerError::route_unavailable(
                    "cluster server is draining and is not accepting new requests".to_string(),
                ));
            }
            if let Some(entry) = state.entries.get_mut(&claim.owner_id) {
                if entry.retiring {
                    return Err(ClusterServerError::route_unavailable(
                        "cluster route generation is retiring; retry against the current definition"
                            .to_string(),
                    ));
                }
                entry.active_requests += 1;
                return Ok(self.lease(claim.owner_id));
            }
        }

        match self
            .ownership
            .acquire_route_claim(
                &self.layout,
                RouteClaimAcquireRequest {
                    claim: claim.clone(),
                },
            )
            .map_err(|error| ClusterServerError::route_unavailable(error.to_string()))?
        {
            RouteClaimTransition::Acquired(_) => {}
            RouteClaimTransition::Existing(existing) => {
                if existing.process != self.process {
                    return Err(ClusterServerError::route_unavailable(format!(
                        "cluster route generation is owned by live process {}; stop the existing server and retry",
                        existing.process.pid
                    )));
                }
            }
            RouteClaimTransition::Busy(busy) => {
                return Err(ClusterServerError::route_unavailable(busy.description))
            }
        }

        let mut state = self.lock_state()?;
        if !state.accepting {
            state.entries.insert(
                claim.owner_id.clone(),
                RouteGenerationEntry {
                    claim: claim.clone(),
                    active_requests: 0,
                    retiring: true,
                },
            );
            drop(state);
            self.release_claim_if_available(&claim.owner_id)?;
            return Err(ClusterServerError::route_unavailable(
                "cluster server entered drain while acquiring the route".to_string(),
            ));
        }
        let entry = state
            .entries
            .entry(claim.owner_id.clone())
            .or_insert(RouteGenerationEntry {
                claim,
                active_requests: 0,
                retiring: false,
            });
        entry.active_requests += 1;
        Ok(self.lease(entry.claim.owner_id.clone()))
    }

    pub(super) fn reconcile_definition(&self, current_hash: &str) {
        let retiring = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            let mut retiring = Vec::new();
            for entry in state.entries.values_mut() {
                if entry.claim.definition_hash != current_hash {
                    let newly_retiring = !entry.retiring;
                    entry.retiring = true;
                    retiring.push((
                        entry.claim.owner_id.clone(),
                        entry.active_requests == 0,
                        newly_retiring,
                    ));
                }
            }
            retiring
        };
        for (owner_id, idle, newly_retiring) in retiring {
            if newly_retiring {
                let _ = self.ownership.retire_route_claim(&self.layout, &owner_id);
            }
            if idle {
                let _ = self.release_claim_if_available(&owner_id);
            }
        }
    }

    pub(super) fn begin_drain(&self) {
        let owner_ids = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.accepting = false;
            for entry in state.entries.values_mut() {
                entry.retiring = true;
            }
            state.entries.keys().cloned().collect::<Vec<_>>()
        };
        for owner_id in &owner_ids {
            let _ = self.ownership.retire_route_claim(&self.layout, owner_id);
        }
    }

    pub(super) async fn finish_drain(&self) -> Result<(), ClusterServerError> {
        loop {
            let idle_owner_ids = self.state.lock().ok().and_then(|state| {
                state
                    .entries
                    .values()
                    .all(|entry| entry.active_requests == 0)
                    .then(|| state.entries.keys().cloned().collect::<Vec<_>>())
            });
            if let Some(owner_ids) = idle_owner_ids {
                for owner_id in owner_ids {
                    self.release_claim_if_available(&owner_id)?;
                }
                if self.active_claim_count() == 0 {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub(super) fn preserve_on_timeout(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.preserve_claims = true;
        }
    }

    pub(super) fn active_claim_count(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.entries.len())
            .unwrap_or_default()
    }

    pub(super) fn active_request_count(&self) -> usize {
        self.state
            .lock()
            .map(|state| {
                state
                    .entries
                    .values()
                    .map(|entry| entry.active_requests)
                    .sum()
            })
            .unwrap_or_default()
    }

    fn release_request(&self, owner_id: &str) {
        let should_release = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            let preserve_claims = state.preserve_claims;
            let Some(entry) = state.entries.get_mut(owner_id) else {
                return;
            };
            entry.active_requests = entry.active_requests.saturating_sub(1);
            entry.retiring && entry.active_requests == 0 && !preserve_claims
        };
        if should_release {
            let _ = self.release_claim_if_available(owner_id);
        }
    }

    fn release_claim_if_available(&self, owner_id: &str) -> Result<bool, ClusterServerError> {
        let released = self
            .ownership
            .release_route_claim(&self.layout, owner_id)
            .map_err(|error| ClusterServerError::route_unavailable(error.to_string()))?
            .is_none();
        if released {
            if let Ok(mut state) = self.state.lock() {
                state.entries.remove(owner_id);
            }
        }
        Ok(released)
    }

    fn lease(&self, owner_id: String) -> RouteRequestLease {
        RouteRequestLease {
            lease: Arc::new(RouteRequestLeaseInner {
                manager: self.clone(),
                owner_id,
            }),
        }
    }

    fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, RouteGenerationState>, ClusterServerError> {
        self.state.lock().map_err(|_| {
            ClusterServerError::route_unavailable(
                "cluster route generation state is unavailable".to_string(),
            )
        })
    }
}

impl super::watch::port::DefinitionRevisionObserver for RouteGenerationManager {
    fn reconcile_definition(&self, hash: &str) {
        RouteGenerationManager::reconcile_definition(self, hash);
    }
}

impl Drop for RouteRequestLeaseInner {
    fn drop(&mut self) {
        self.manager.release_request(&self.owner_id);
    }
}

impl std::fmt::Debug for RouteRequestLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RouteRequestLease")
            .field("owner_id", &self.lease.owner_id)
            .finish()
    }
}
