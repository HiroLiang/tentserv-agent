#![forbid(unsafe_code)]

//! Shared Tentgent domain, infrastructure ports, runtime layout, and feature
//! use cases.

pub mod capabilities;
pub mod features;
pub mod foundation;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
