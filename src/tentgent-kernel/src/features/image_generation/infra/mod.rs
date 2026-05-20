//! Standard image-generation infrastructure implementations.

mod resolver;
mod runtime;

pub use resolver::StdImageGenerationModelResolver;
pub use runtime::PythonImageGenerationOnceRuntimeClient;
