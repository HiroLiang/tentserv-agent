//! Managed store maintenance use case ports.

use crate::features::store::domain::StoreGcOutcome;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreGcRequest {
    pub layout: RuntimeLayoutInput,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreGcResult {
    pub layout: RuntimeLayout,
    pub outcome: StoreGcOutcome,
}

pub trait StoreGcUseCase {
    fn gc_stores(&self, request: StoreGcRequest) -> KernelResult<StoreGcResult>;
}
