//! Long-running Tentgent daemon application host.

pub mod app;
pub mod bootstrap;
pub mod cloud_server;
pub mod handlers;
pub mod kernel;
pub mod local_server;
pub mod runtime;
pub mod transport;

pub use app::{DaemonApp, DaemonAppState, DaemonServices};
pub use bootstrap::{bootstrap_daemon_app, DaemonBootstrapConfig, LoggingConfig, RestConfig};
pub use kernel::KernelComponents;
