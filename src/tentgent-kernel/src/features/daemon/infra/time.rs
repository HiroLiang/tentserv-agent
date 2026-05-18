use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::daemon::ports::DaemonClock;
use crate::foundation::error::KernelResult;

use super::error::daemon_store_error;

/// System UTC clock for daemon process records.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemDaemonClock;

impl DaemonClock for SystemDaemonClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|err| daemon_store_error(format!("format daemon timestamp failed: {err}")))
    }
}
