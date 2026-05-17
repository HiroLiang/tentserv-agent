//! Daemon feature package.

pub mod domain;
pub mod infra;
pub mod ports;
pub mod usecases;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
