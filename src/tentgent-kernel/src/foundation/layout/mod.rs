//! Runtime-home layout structures, ports, and infrastructure.

pub mod domain;
pub mod infra;
pub mod ports;

#[cfg(test)]
mod tests;

pub use domain::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};
pub use infra::{StdRuntimeLayoutResolver, DATA_ROOT_ENV, HOME_ENV};
pub use ports::RuntimeLayoutResolver;
