use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::server::ports::ServerClock;
use crate::foundation::error::KernelResult;

use super::error::server_store_error;

/// System UTC clock for server records.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemServerClock;

impl ServerClock for SystemServerClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|err| server_store_error(format!("format server timestamp failed: {err}")))
    }
}
