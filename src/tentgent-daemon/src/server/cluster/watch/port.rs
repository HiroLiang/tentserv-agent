use std::{future::Future, pin::Pin, time::Instant};

use super::super::{cache::ClusterDefinitionCache, error::ClusterServerError};

pub(super) trait DefinitionRevisionProbe: Send + Sync {
    fn refresh(
        &self,
        cache: &ClusterDefinitionCache,
        force_hash: bool,
    ) -> Result<String, ClusterServerError>;
}

pub(crate) trait DefinitionRevisionObserver: Send + Sync {
    fn reconcile_definition(&self, hash: &str);
}

pub(super) type DefinitionWatchTickFuture<'a> =
    Pin<Box<dyn Future<Output = Option<Instant>> + Send + 'a>>;

pub(super) trait DefinitionWatchTickSource: Send {
    fn origin(&self) -> Instant;
    fn next_tick(&mut self) -> DefinitionWatchTickFuture<'_>;
}

pub(super) type CandidatePrepareFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), ClusterServerError>> + Send + 'a>>;

pub(super) trait CandidatePreparer: Send + Sync {
    fn prepare<'a>(
        &'a self,
        state: &'a super::super::state::ClusterServerState,
        snapshot: &'a super::super::cache::ClusterDefinitionSnapshot,
    ) -> CandidatePrepareFuture<'a>;
}
