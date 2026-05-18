use miette::{IntoDiagnostic, Result};
use tentgent_kernel::features::daemon::infra::StdDaemonKernel;

use crate::{bootstrap::DaemonBootstrapConfig, kernel::KernelComponents};

pub struct DaemonServices {
    kernel: KernelComponents,
}

impl DaemonServices {
    pub fn bootstrap(config: &DaemonBootstrapConfig) -> Result<Self> {
        Ok(Self {
            kernel: KernelComponents::bootstrap(config).into_diagnostic()?,
        })
    }

    pub fn kernel(&self) -> &KernelComponents {
        &self.kernel
    }

    pub fn daemon(&self) -> &StdDaemonKernel {
        self.kernel.daemon()
    }
}
