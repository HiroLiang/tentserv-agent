use super::{
    domain::DefinitionWatchSchedule,
    port::{CandidatePrepareFuture, CandidatePreparer, DefinitionWatchTickSource},
    strategy::HybridWatchStrategy,
};
use crate::server::cluster::{
    cache::{ClusterDefinitionSnapshot, DefinitionCandidate},
    leases::RouteGenerationManager,
    startup::{preload_route, prepare_snapshot_with},
    state::ClusterServerState,
};
use tokio::sync::watch;

pub(super) struct RuntimeCandidatePreparer;
impl CandidatePreparer for RuntimeCandidatePreparer {
    fn prepare<'a>(
        &'a self,
        state: &'a ClusterServerState,
        snapshot: &'a ClusterDefinitionSnapshot,
    ) -> CandidatePrepareFuture<'a> {
        Box::pin(prepare_snapshot_with(state, snapshot, true, preload_route))
    }
}

struct StagedRevision {
    routes: RouteGenerationManager,
    hash: String,
    promoted: bool,
}
impl Drop for StagedRevision {
    fn drop(&mut self) {
        if !self.promoted {
            self.routes.discard_staged(&self.hash);
        }
    }
}

pub(super) async fn run_eager_watcher(
    state: ClusterServerState,
    mut cancelled: watch::Receiver<bool>,
    mut ticks: Box<dyn DefinitionWatchTickSource>,
    schedule: DefinitionWatchSchedule,
    preparer: &dyn CandidatePreparer,
) {
    let mut strategy = HybridWatchStrategy::new_at(schedule, ticks.origin());
    let mut attempted: Option<DefinitionCandidate> = None;
    state.reload_status.set("idle", None, None);
    loop {
        if *cancelled.borrow() {
            return;
        }
        let tick = tokio::select! {
            biased;
            _ = cancelled.changed() => return,
            tick = ticks.next_tick() => tick,
        };
        let Some(now) = tick else {
            return;
        };
        // Cross-process contention may defer release. Retry idle retired claims
        // even when no new candidate appears; staged claims remain protected.
        let _ = state.definitions.with_committed(|snapshot| {
            state.routes.reconcile_definition(&snapshot.hash);
            Ok(())
        });
        let candidate = match state.definitions.candidate(strategy.should_force_hash(now)) {
            Ok(Some(candidate)) => candidate,
            Ok(None) => {
                state.reload_status.set("idle", None, None);
                continue;
            }
            Err(error) => {
                state
                    .reload_status
                    .set("failed", None, Some(error.to_string()));
                continue;
            }
        };
        if attempted
            .as_ref()
            .is_some_and(|previous| previous.same_attempt(&candidate))
        {
            continue;
        }
        attempted = Some(candidate.clone());
        state
            .reload_status
            .set("preparing", Some(&candidate.snapshot().hash), None);
        let mut staged = StagedRevision {
            routes: state.routes.clone(),
            hash: candidate.snapshot().hash.clone(),
            promoted: false,
        };
        let preparation = preparer.prepare(&state, candidate.snapshot());
        tokio::pin!(preparation);
        let result = tokio::select! {
            biased;
            _ = cancelled.changed() => {
                // Do not drop accepted work simply because its observer stopped.
                // The host bounds this await and aborts the watcher after 30s.
                state.begin_drain();
                let _ = (&mut preparation).await;
                return;
            }
            result = &mut preparation => result,
        };
        if *cancelled.borrow() || !state.routes.is_accepting() {
            return;
        }
        let promoted = result.and_then(|()| {
            state.definitions.promote_with(&candidate, |snapshot| {
                state.routes.commit_staged(&snapshot.hash)
            })
        });
        match promoted {
            Ok(true) => {
                staged.promoted = true;
                state.reload_status.set("idle", None, None);
            }
            Ok(false) => state.reload_status.set(
                "superseded",
                Some(&staged.hash),
                Some("candidate changed before promotion; keeping committed routes".into()),
            ),
            Err(error) if error.is_retryable_transition() => {
                // A rollback can meet its own previous stream's retiring claim.
                // Retry only known ownership contention/drain, once per watcher
                // tick; terminal loading/validation and unknown completion stay
                // memoized so native loaders cannot be retried accidentally.
                attempted = None;
                state
                    .reload_status
                    .set("waiting", Some(&staged.hash), Some(error.to_string()));
            }
            Err(error) => {
                tracing::warn!(%error, candidate_hash = %staged.hash, "cluster reload failed; keeping committed routes");
                state
                    .reload_status
                    .set("failed", Some(&staged.hash), Some(error.to_string()));
            }
        }
        // Accepted B completes before C starts; C is always re-read from disk.
        // No cache/route mutex or transition permit spans any preload await.
    }
}

#[cfg(test)]
mod tests;
