//! Cross-platform filesystem primitives.

mod atomic_write;

pub use atomic_write::{atomic_write, sync_directory};
