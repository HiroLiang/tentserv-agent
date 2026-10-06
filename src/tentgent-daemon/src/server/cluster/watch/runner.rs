use std::{sync::Arc, time::Instant};

use tokio::sync::watch;

use super::{
    domain::DefinitionWatchSchedule,
    port::{
        DefinitionRevisionObserver, DefinitionRevisionProbe, DefinitionWatchTickFuture,
        DefinitionWatchTickSource,
    },
    probes::PlatformDefinitionRevisionProbe,
    strategy::HybridWatchStrategy,
};
use crate::server::cluster::{cache::ClusterDefinitionCache, state::ClusterServerState};

pub(crate) async fn run_definition_watcher(
    state: ClusterServerState,
    cancelled: watch::Receiver<bool>,
) {
    let schedule = DefinitionWatchSchedule::default();
    if state.config.load_mode == tentgent_kernel::features::server::options::LoadMode::Eager {
        super::reload::run_eager_watcher(
            state,
            cancelled,
            Box::new(TokioDefinitionWatchTickSource::new(
                schedule.metadata_interval,
            )),
            schedule,
            &super::reload::RuntimeCandidatePreparer,
        )
        .await;
        return;
    }
    run_definition_watcher_with_dependencies(
        state.definitions,
        cancelled,
        DefinitionWatcherDependencies {
            probe: Arc::new(PlatformDefinitionRevisionProbe),
            observer: Arc::new(state.routes),
            ticks: Box::new(TokioDefinitionWatchTickSource::new(
                schedule.metadata_interval,
            )),
            schedule,
        },
    )
    .await;
}

pub(super) struct DefinitionWatcherDependencies {
    pub probe: Arc<dyn DefinitionRevisionProbe>,
    pub observer: Arc<dyn DefinitionRevisionObserver>,
    pub ticks: Box<dyn DefinitionWatchTickSource>,
    pub schedule: DefinitionWatchSchedule,
}

pub(super) async fn run_definition_watcher_with_dependencies(
    cache: ClusterDefinitionCache,
    mut cancelled: watch::Receiver<bool>,
    mut dependencies: DefinitionWatcherDependencies,
) {
    if *cancelled.borrow() {
        return;
    }
    let mut strategy =
        HybridWatchStrategy::new_at(dependencies.schedule, dependencies.ticks.origin());
    loop {
        tokio::select! {
            tick = dependencies.ticks.next_tick() => {
                let Some(now) = tick else {
                    return;
                };
                let force_hash = strategy.should_force_hash(now);
                if let Ok(hash) = dependencies.probe.refresh(&cache, force_hash) {
                    dependencies.observer.reconcile_definition(&hash);
                }
            }
            changed = cancelled.changed() => {
                if changed.is_err() || *cancelled.borrow() {
                    return;
                }
            }
        }
    }
}

struct TokioDefinitionWatchTickSource {
    origin: Instant,
    interval: tokio::time::Interval,
}

impl TokioDefinitionWatchTickSource {
    fn new(interval: std::time::Duration) -> Self {
        let mut interval = tokio::time::interval(interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Self {
            origin: Instant::now(),
            interval,
        }
    }
}

impl DefinitionWatchTickSource for TokioDefinitionWatchTickSource {
    fn origin(&self) -> Instant {
        self.origin
    }

    fn next_tick(&mut self) -> DefinitionWatchTickFuture<'_> {
        Box::pin(async move {
            self.interval.tick().await;
            Some(Instant::now())
        })
    }
}

#[cfg(test)]
mod tests;
