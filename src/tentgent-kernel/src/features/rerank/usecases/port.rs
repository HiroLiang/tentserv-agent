//! Rerank use case ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::rerank::domain::{RerankInput, RerankRequest, RerankResponse};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by rerank use cases that execute runtime work.
pub type RerankUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// Request for preparing one rerank request without executing inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RerankPreparationRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub model_selector: ModelRefSelector,
    pub input: RerankInput,
}

/// Result of resolving layout, runtime, model, and the runtime request.
#[derive(Debug, Clone, PartialEq)]
pub struct RerankPreparationResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub model: ModelInspection,
    pub request: RerankRequest,
}

/// Result of executing one prepared rerank request.
#[derive(Debug, Clone, PartialEq)]
pub struct RerankExecutionResult {
    pub prepared: RerankPreparationResult,
    pub response: RerankResponse,
}

/// Use-case boundary for preparing rerank runtime requests.
pub trait RerankPreparationUseCase {
    /// Resolves the selected target and builds the canonical runtime request.
    fn prepare_rerank(
        &self,
        request: RerankPreparationRequest,
    ) -> KernelResult<RerankPreparationResult>;
}

/// Use-case boundary for one-shot rerank inference.
pub trait RerankUseCase {
    /// Resolves target/runtime and returns ranked scores for the provided input.
    fn rerank(
        &'_ self,
        request: RerankPreparationRequest,
    ) -> RerankUseCaseFuture<'_, RerankExecutionResult>;
}
