mod services;
mod state;

pub use services::DaemonServices;
pub use state::DaemonAppState;

use std::sync::Arc;

use miette::Result;

use crate::transport::rest::RestEntrypoint;

pub struct DaemonApp {
    state: Arc<DaemonAppState>,
}

impl DaemonApp {
    pub fn new(state: DaemonAppState) -> Self {
        Self {
            state: Arc::new(state),
        }
    }

    pub fn state(&self) -> &DaemonAppState {
        self.state.as_ref()
    }

    pub fn shared_state(&self) -> Arc<DaemonAppState> {
        Arc::clone(&self.state)
    }

    pub async fn run_until_shutdown(self) -> Result<()> {
        RestEntrypoint::new(self.state.rest_config().clone())
            .run(self.state)
            .await
    }
}
