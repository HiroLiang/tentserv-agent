use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::session::ports::SessionClock;
use crate::foundation::error::KernelResult;

use super::error::session_store_error;

/// System clock for filesystem-backed session records.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemSessionClock;

impl SessionClock for SystemSessionClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|err| session_store_error(format!("format session timestamp failed: {err}")))
    }
}
