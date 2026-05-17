//! Shared network string formatting helpers.

pub mod domain;

#[cfg(test)]
mod tests;

pub use domain::{format_host_for_url_authority, http_url_from_host_port};
