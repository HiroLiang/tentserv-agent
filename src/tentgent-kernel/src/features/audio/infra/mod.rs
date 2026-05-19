//! Standard audio infrastructure implementations.

mod resolver;
mod runtime;

pub use resolver::StdAudioTranscriptionModelResolver;
pub use runtime::PythonAudioTranscriptionBatchRuntimeClient;
