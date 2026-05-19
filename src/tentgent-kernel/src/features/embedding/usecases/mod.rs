//! Embedding use cases.

mod embedding;
mod port;

pub use embedding::StdEmbeddingUseCase;
pub use port::{
    EmbeddingExecutionResult, EmbeddingPreparationRequest, EmbeddingPreparationResult,
    EmbeddingPreparationUseCase, EmbeddingUseCase, EmbeddingUseCaseFuture,
};
