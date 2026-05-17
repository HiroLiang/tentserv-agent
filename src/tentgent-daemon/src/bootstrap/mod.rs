mod config;
mod logging;

pub use config::{DaemonBootstrapConfig, LoggingConfig, RestConfig};
pub use logging::{init_logging, LoggingRuntime};

use miette::{IntoDiagnostic, Result};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

use crate::app::{DaemonApp, DaemonAppState, DaemonServices};

pub fn bootstrap_daemon_app(config: DaemonBootstrapConfig) -> Result<DaemonApp> {
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: config.home.clone(),
            data_root_dir: None,
        })
        .into_diagnostic()?;
    let logging = init_logging(&config.logging, &layout.logs_dir)?;
    tracing::info!(
        home_dir = %layout.home_dir.display(),
        logs_dir = %layout.logs_dir.display(),
        "daemon bootstrap starting"
    );

    let services = DaemonServices::bootstrap(&config)?;
    let state = DaemonAppState::new(services, logging, config.rest);

    Ok(DaemonApp::new(state))
}
