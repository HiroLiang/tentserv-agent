use std::fs;

use crate::features::daemon::domain::DaemonStoreLayout;
use crate::features::daemon::ports::DaemonStoreLayoutInitializer;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Filesystem layout initializer for daemon runtime metadata and logs.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdDaemonStoreLayoutInitializer;

impl DaemonStoreLayoutInitializer for StdDaemonStoreLayoutInitializer {
    fn ensure_daemon_store_layout(&self, layout: &DaemonStoreLayout) -> KernelResult<()> {
        fs::create_dir_all(&layout.runtime_dir).map_err(|err| {
            path_error("create daemon runtime directory", &layout.runtime_dir, err)
        })?;
        fs::create_dir_all(&layout.log_dir)
            .map_err(|err| path_error("create daemon log directory", &layout.log_dir, err))?;
        Ok(())
    }
}
