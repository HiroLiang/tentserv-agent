//! Runtime layout package ports.

use crate::foundation::error::KernelResult;

use super::domain::{RuntimeLayout, RuntimeLayoutInput};

pub trait RuntimeLayoutResolver {
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout>;
}
