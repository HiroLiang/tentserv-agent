//! Infrastructure adapters for embedding feature ports.

mod resolver;
mod runtime;

pub use resolver::StdEmbeddingModelResolver;
pub use runtime::PythonEmbeddingOnceRuntimeClient;
