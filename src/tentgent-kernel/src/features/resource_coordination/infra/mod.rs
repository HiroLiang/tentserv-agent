//! File-backed resource coordination adapters.

mod file_coordinator;
mod holder_metadata;
mod layout;

pub use file_coordinator::FileResourceCoordinator;
pub use layout::{coordination_lock_path, coordination_root};

#[cfg(test)]
mod tests;
