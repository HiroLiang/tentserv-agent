//! Standard image-generation infrastructure implementations.

mod resolver;
mod runtime;

pub use resolver::{StdImageGenerationAdapterResolver, StdImageGenerationModelResolver};
pub use runtime::PythonImageGenerationOnceRuntimeClient;
