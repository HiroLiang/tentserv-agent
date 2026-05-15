//! Platform package ports.

use crate::foundation::error::KernelResult;

use super::domain::PlatformFacts;

pub trait PlatformProbe {
    fn probe(&self) -> KernelResult<PlatformFacts>;
}
