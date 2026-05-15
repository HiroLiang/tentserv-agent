#![forbid(unsafe_code)]

//! Internal architecture landing zone for Tentgent package shape and domain
//! data objects.
//!
//! This crate starts as a compile-only shell. Existing behavior remains in
//! `tentgent-core` until each coherent bundle is moved deliberately.

pub mod capabilities;
pub mod features;
pub mod foundation;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
