use miette::{IntoDiagnostic, Result};
use tentgent_kernel::features::daemon::infra::{StdDaemonKernel, DEFAULT_DAEMON_PROBE_TIMEOUT};

use crate::bootstrap::DaemonBootstrapConfig;

pub struct DaemonServices {
    daemon: StdDaemonKernel,
}

impl DaemonServices {
    pub fn bootstrap(_config: &DaemonBootstrapConfig) -> Result<Self> {
        Ok(Self {
            daemon: StdDaemonKernel::new(DEFAULT_DAEMON_PROBE_TIMEOUT).into_diagnostic()?,
        })
    }

    pub fn daemon(&self) -> &StdDaemonKernel {
        &self.daemon
    }
}
