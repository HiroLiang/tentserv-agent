mod router;

pub mod error;
pub mod response;
pub mod state;

#[cfg(test)]
mod tests;

pub use router::build_router;

use std::sync::Arc;

use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::daemon::{
    domain::DaemonBind,
    usecases::{
        DaemonClearProcessRequest, DaemonLifecycleUseCase, DaemonPrepareRunRequest,
        DaemonRecordProcessStartRequest,
    },
};
use tentgent_kernel::foundation::layout::LayoutResolveMode;
use tokio::net::TcpListener;

use crate::{app::DaemonAppState, bootstrap::RestConfig};

use self::state::RestState;

pub struct RestEntrypoint {
    config: RestConfig,
}

impl RestEntrypoint {
    pub fn new(config: RestConfig) -> Self {
        Self { config }
    }

    pub async fn run(self, state: Arc<DaemonAppState>) -> Result<()> {
        if !self.config.enabled {
            tracing::info!("daemon rest transport disabled");
            return Ok(());
        }

        let daemon = state.services().daemon().usecase();
        let prepared = daemon
            .prepare_run(DaemonPrepareRunRequest {
                layout: state.layout_input(LayoutResolveMode::Create),
                host: Some(self.config.host.clone()),
                port: Some(self.config.port),
                token_enabled: false,
                allow_unsafe_bind: false,
            })
            .into_diagnostic()?;
        for warning in prepared.bind_warnings {
            tracing::warn!(warning = %warning, "daemon rest bind warning");
        }

        let listener = TcpListener::bind((prepared.bind.host.as_str(), prepared.bind.port))
            .await
            .map_err(|err| {
                miette!(
                    "failed to bind daemon REST listener on {}:{}: {err}",
                    prepared.bind.host,
                    prepared.bind.port
                )
            })?;
        let local_addr = listener.local_addr().into_diagnostic()?;
        let pid = std::process::id();
        let recorded = daemon
            .record_process_start(DaemonRecordProcessStartRequest {
                layout: state.layout_input(LayoutResolveMode::ReadOnly),
                pid,
                bind: DaemonBind {
                    host: prepared.bind.host.clone(),
                    port: local_addr.port(),
                },
            })
            .into_diagnostic()?;

        tracing::info!(
            host = %prepared.bind.host,
            port = local_addr.port(),
            pid,
            runtime_home = %recorded.inspection.home_dir.display(),
            "daemon rest transport listening"
        );

        let app = build_router(RestState::new(Arc::clone(&state)));
        let serve_result = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .into_diagnostic();
        let clear_result = daemon
            .clear_process_if_matches(DaemonClearProcessRequest {
                layout: state.layout_input(LayoutResolveMode::ReadOnly),
                expected_pid: Some(pid),
            })
            .into_diagnostic();

        if let Err(err) = clear_result {
            tracing::warn!(error = %err, "failed to clear daemon process metadata");
        }

        serve_result
    }
}

async fn shutdown_signal() {
    if let Err(err) = tokio::signal::ctrl_c().await {
        tracing::warn!(error = %err, "failed to listen for ctrl-c");
    }
}
