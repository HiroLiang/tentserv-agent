//! Audio use case boundaries.

pub mod port;
mod speech;
mod transcription;

pub use port::{
    AudioSpeechExecutionResult, AudioSpeechPreparationRequest, AudioSpeechPreparationResult,
    AudioSpeechPreparationUseCase, AudioSpeechUseCase, AudioTranscriptionExecutionResult,
    AudioTranscriptionPreparationRequest, AudioTranscriptionPreparationResult,
    AudioTranscriptionPreparationUseCase, AudioTranscriptionUseCase, AudioUseCaseFuture,
};
pub use speech::StdAudioSpeechUseCase;
pub use transcription::StdAudioTranscriptionUseCase;
