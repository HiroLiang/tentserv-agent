//! Long-running Tentgent daemon application host.

pub mod app;
pub mod bootstrap;
pub mod handlers;
pub mod kernel;
mod provider_compat;
pub mod runtime;
pub mod server;
mod time;
pub mod transport;

pub use app::{DaemonApp, DaemonAppState, DaemonServices};
pub use bootstrap::{bootstrap_daemon_app, DaemonBootstrapConfig, LoggingConfig, RestConfig};
pub use kernel::KernelComponents;
