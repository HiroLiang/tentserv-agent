//! Standard audio infrastructure implementations.

mod resolver;
mod runtime;

pub use resolver::StdAudioSpeechModelResolver;
pub use resolver::StdAudioTranscriptionModelResolver;
pub use runtime::PythonAudioSpeechOnceRuntimeClient;
pub use runtime::PythonAudioTranscriptionBatchRuntimeClient;
