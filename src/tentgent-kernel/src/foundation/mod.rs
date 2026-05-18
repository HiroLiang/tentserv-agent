//! Shared kernel primitives used by feature packages.
//!
//! This package should own runtime layout, environment resolution, common
//! identifiers, and shared error/result types. It should not own user-visible
//! workflows.

pub mod error;
pub mod layout;
pub mod net;
pub mod platform;
