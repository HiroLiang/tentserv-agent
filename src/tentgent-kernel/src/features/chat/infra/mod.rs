//! Standard chat infrastructure adapters.

mod resolver;
mod runtime;

pub use resolver::{StdChatAdapterResolver, StdChatModelResolver};
pub use runtime::PythonChatOnceRuntimeClient;

#[cfg(test)]
mod tests;
