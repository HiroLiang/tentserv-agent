//! Chat use case boundaries.

mod completion;
pub mod port;

#[cfg(test)]
mod tests;

pub use completion::StdChatUseCase;
pub use port::{
    ChatCompletionResult, ChatCompletionUseCase, ChatPreparationRequest, ChatPreparationResult,
    ChatPreparationUseCase, ChatStreamingUseCase, ChatTargetSelection, ChatUseCaseFuture,
};
