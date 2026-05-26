//! Infrastructure adapters for rerank feature ports.

mod resolver;
mod runtime;

pub use resolver::StdRerankModelResolver;
pub use runtime::PythonRerankModelRuntimeClient;
