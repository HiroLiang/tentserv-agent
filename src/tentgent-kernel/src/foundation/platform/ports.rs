//! Platform package ports.

use crate::foundation::error::KernelResult;

use super::domain::PlatformFacts;

/// Reads host operating-system and hardware facts for kernel decisions.
pub trait PlatformProbe {
    /// Returns a fresh snapshot of the current host platform.
    fn probe(&self) -> KernelResult<PlatformFacts>;
}
