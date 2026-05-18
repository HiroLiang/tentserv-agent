//! Daemon filesystem, process, bind-safety, launcher, and readiness infrastructure.

mod bind;
mod composition;
mod error;
mod launcher;
mod layout;
mod process;
mod readiness;
mod store;
mod time;

#[cfg(test)]
mod tests;

pub use bind::StdDaemonBindSafetyChecker;
pub use composition::{daemon_runtime_layout_input, StdDaemonKernel};
pub use launcher::StdDaemonDetachedLauncher;
pub use layout::StdDaemonStoreLayoutInitializer;
pub use process::{StdDaemonProcessController, StdDaemonProcessProbe};
pub use readiness::{ReqwestDaemonHttpReadinessProbe, DEFAULT_DAEMON_PROBE_TIMEOUT};
pub use store::FileDaemonStateStore;
pub use time::SystemDaemonClock;
