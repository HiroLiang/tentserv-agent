use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::model::ports::ModelClock;
use crate::foundation::error::KernelResult;

use super::error::model_store_error;

/// System UTC clock for model records.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemModelClock;

impl ModelClock for SystemModelClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|err| model_store_error(format!("format model timestamp failed: {err}")))
    }
}
