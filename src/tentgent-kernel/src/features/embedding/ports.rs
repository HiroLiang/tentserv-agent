//! Embedding feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{EmbeddingRequest, EmbeddingResponse, EmbeddingRuntimeTarget};

pub type EmbeddingPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: EmbeddingRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: EmbeddingRequest,
}

/// Boundary for resolving a model selector into an embedding-capable runtime target.
pub trait EmbeddingModelResolver {
    /// Resolves a model ref or unique prefix and maps it to an embedding runtime target.
    fn resolve_embedding_model(
        &self,
        request: EmbeddingModelResolveRequest,
    ) -> KernelResult<EmbeddingModelResolveResult>;
}

/// Boundary for executing a prepared embedding request against the selected runtime.
pub trait EmbeddingRuntimeClient {
    /// Returns embeddings for one prepared request.
    fn embed<'a>(
        &'a self,
        request: EmbeddingRuntimeRequest,
    ) -> EmbeddingPortFuture<'a, EmbeddingResponse>;
}
