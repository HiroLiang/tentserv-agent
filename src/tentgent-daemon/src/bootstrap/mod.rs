mod config;
mod logging;

pub use config::{DaemonBootstrapConfig, LoggingConfig, RestConfig};
pub use logging::init_logging;

use miette::Result;

use crate::app::{DaemonApp, DaemonAppState, DaemonServices};

pub fn bootstrap_daemon_app(config: DaemonBootstrapConfig) -> Result<DaemonApp> {
    init_logging(&config.logging)?;

    let services = DaemonServices::bootstrap(&config)?;
    let state = DaemonAppState::new(services, config.rest);

    Ok(DaemonApp::new(state))
}
