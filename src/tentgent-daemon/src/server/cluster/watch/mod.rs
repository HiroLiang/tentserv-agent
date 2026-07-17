//! Cancellable hybrid cluster definition watcher.

mod domain;
pub(super) mod port;
mod probes;
mod runner;
mod strategy;

pub(super) use runner::run_definition_watcher;
