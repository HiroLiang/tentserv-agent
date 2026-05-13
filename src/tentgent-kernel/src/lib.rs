#![forbid(unsafe_code)]

//! Internal architecture landing zone for Tentgent domain, layout, stores,
//! use cases, runtime adapters, and machine-local capability state.
//!
//! This crate starts as a compile-only shell. Existing behavior remains in
//! `tentgent-core` until each coherent bundle is moved behind compatibility
//! adapters.

pub mod capabilities;
pub mod features;
pub mod foundation;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
