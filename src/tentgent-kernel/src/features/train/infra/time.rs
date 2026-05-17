use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::features::train::ports::TrainClock;
use crate::foundation::error::KernelResult;

use super::error::train_store_error;

/// UTC wall clock used for train records.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTrainClock;

impl TrainClock for SystemTrainClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|err| train_store_error(format!("format train timestamp failed: {err}")))
    }
}
