use std::sync::Arc;

use crate::app::DaemonAppState;

#[derive(Clone)]
pub struct RestState {
    app: Arc<DaemonAppState>,
}

impl RestState {
    pub fn new(app: Arc<DaemonAppState>) -> Self {
        Self { app }
    }

    pub fn app(&self) -> &DaemonAppState {
        self.app.as_ref()
    }
}
