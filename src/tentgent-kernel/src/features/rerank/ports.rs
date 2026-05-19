//! Rerank feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{RerankRequest, RerankResponse, RerankRuntimeTarget};

pub type RerankPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RerankModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RerankModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: RerankRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: RerankRequest,
}

/// Boundary for resolving a model selector into a rerank-capable runtime target.
pub trait RerankModelResolver {
    /// Resolves a model ref or unique prefix and maps it to a rerank runtime target.
    fn resolve_rerank_model(
        &self,
        request: RerankModelResolveRequest,
    ) -> KernelResult<RerankModelResolveResult>;
}

/// Boundary for executing a prepared rerank request against the selected runtime.
pub trait RerankRuntimeClient {
    /// Returns ranked scores for one prepared request.
    fn rerank(&'_ self, request: RerankRuntimeRequest) -> RerankPortFuture<'_, RerankResponse>;
}
