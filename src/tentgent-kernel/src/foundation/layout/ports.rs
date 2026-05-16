//! Runtime layout package ports.

use crate::foundation::error::KernelResult;

use super::domain::{RuntimeLayout, RuntimeLayoutInput};

/// Resolves Tentgent runtime-home paths from explicit input and environment.
pub trait RuntimeLayoutResolver {
    /// Builds the canonical runtime layout and creates directories only when the input requests it.
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout>;
}
