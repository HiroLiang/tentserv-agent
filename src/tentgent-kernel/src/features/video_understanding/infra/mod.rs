//! Infrastructure adapters for video-understanding.

mod resolver;
mod runtime;

pub use resolver::StdVideoUnderstandingModelResolver;
pub use runtime::PythonVideoUnderstandingOnceRuntimeClient;
