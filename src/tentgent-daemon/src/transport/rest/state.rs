use std::sync::Arc;

use crate::app::DaemonAppState;

use super::security::DaemonSecurityConfig;

#[derive(Clone)]
pub struct RestState {
    app: Arc<DaemonAppState>,
    security: DaemonSecurityConfig,
}

impl RestState {
    pub fn new(app: Arc<DaemonAppState>) -> Self {
        Self {
            app,
            security: DaemonSecurityConfig::from_env(),
        }
    }

    pub fn with_security(app: Arc<DaemonAppState>, security: DaemonSecurityConfig) -> Self {
        Self { app, security }
    }

    pub fn app(&self) -> &DaemonAppState {
        self.app.as_ref()
    }

    pub fn security(&self) -> &DaemonSecurityConfig {
        &self.security
    }
}
