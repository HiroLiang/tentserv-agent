use miette::Result;

use crate::{app::DaemonAppState, bootstrap::RestConfig};

pub struct RestEntrypoint {
    config: RestConfig,
}

impl RestEntrypoint {
    pub fn new(config: RestConfig) -> Self {
        Self { config }
    }

    pub async fn run(self, state: &DaemonAppState) -> Result<()> {
        let _ = state.services().daemon();

        if !self.config.enabled {
            tracing::info!("daemon rest transport disabled");
            return Ok(());
        }

        tracing::info!(
            host = %self.config.host,
            port = self.config.port,
            "daemon rest transport bootstrap placeholder"
        );
        Ok(())
    }
}
