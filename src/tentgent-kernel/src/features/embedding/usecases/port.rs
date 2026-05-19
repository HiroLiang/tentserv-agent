//! Embedding use case ports.

use std::{future::Future, pin::Pin};

use crate::features::embedding::domain::{EmbeddingInput, EmbeddingRequest, EmbeddingResponse};
use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by embedding use cases that execute runtime work.
pub type EmbeddingUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Request for preparing one embedding request without executing inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingPreparationRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub model_selector: ModelRefSelector,
    pub input: EmbeddingInput,
}

/// Result of resolving layout, runtime, model, and the runtime request.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingPreparationResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub model: ModelInspection,
    pub request: EmbeddingRequest,
}

/// Result of executing one prepared embedding request.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingExecutionResult {
    pub prepared: EmbeddingPreparationResult,
    pub response: EmbeddingResponse,
}

/// Use-case boundary for preparing embedding runtime requests.
pub trait EmbeddingPreparationUseCase {
    /// Resolves the selected target and builds the canonical runtime request.
    fn prepare_embedding(
        &self,
        request: EmbeddingPreparationRequest,
    ) -> KernelResult<EmbeddingPreparationResult>;
}

/// Use-case boundary for one-shot embedding inference.
pub trait EmbeddingUseCase {
    /// Resolves target/runtime and returns embeddings for the provided input.
    fn embed(
        &'_ self,
        request: EmbeddingPreparationRequest,
    ) -> EmbeddingUseCaseFuture<'_, EmbeddingExecutionResult>;
}
