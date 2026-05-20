//! Image generation feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{
    ImageGenerationRequest, ImageGenerationResponse, ImageGenerationRuntimeTarget,
};

pub type ImageGenerationPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageGenerationModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageGenerationModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: ImageGenerationRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageGenerationRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: ImageGenerationRequest,
}

/// Boundary for resolving a model selector into an image-generation runtime target.
pub trait ImageGenerationModelResolver {
    /// Resolves a model ref or unique prefix and maps it to an image-generation target.
    fn resolve_image_generation_model(
        &self,
        request: ImageGenerationModelResolveRequest,
    ) -> KernelResult<ImageGenerationModelResolveResult>;
}

/// Boundary for executing a prepared image-generation request.
pub trait ImageGenerationRuntimeClient {
    /// Generates one image file for one prepared request.
    fn generate_image(
        &'_ self,
        request: ImageGenerationRuntimeRequest,
    ) -> ImageGenerationPortFuture<'_, ImageGenerationResponse>;
}
