mod services;
mod state;

pub use services::DaemonServices;
pub use state::DaemonAppState;

use miette::Result;

use crate::transport::rest::RestEntrypoint;

pub struct DaemonApp {
    state: DaemonAppState,
}

impl DaemonApp {
    pub fn new(state: DaemonAppState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &DaemonAppState {
        &self.state
    }

    pub async fn run_until_shutdown(self) -> Result<()> {
        RestEntrypoint::new(self.state.rest_config().clone())
            .run(&self.state)
            .await
    }
}
