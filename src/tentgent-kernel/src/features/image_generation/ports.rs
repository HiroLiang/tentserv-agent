//! Image generation feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::adapter::domain::{
    AdapterCompatibilityTarget, AdapterInspection, AdapterRefSelector,
};
use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{
    ImageGenerationRequest, ImageGenerationResponse, ImageGenerationRuntimeTarget,
    ResolvedImageGenerationAdapter,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageGenerationAdapterResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: AdapterRefSelector,
    pub target: AdapterCompatibilityTarget,
    pub lora_scale: crate::features::adapter::domain::LoraScale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageGenerationAdapterResolveResult {
    pub layout: RuntimeLayout,
    pub adapter: AdapterInspection,
    pub target: ResolvedImageGenerationAdapter,
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

/// Boundary for resolving and validating an adapter for a selected image target.
pub trait ImageGenerationAdapterResolver {
    /// Resolves an adapter ref or unique prefix and validates backend/model compatibility.
    fn resolve_image_generation_adapter(
        &self,
        request: ImageGenerationAdapterResolveRequest,
    ) -> KernelResult<ImageGenerationAdapterResolveResult>;
}

/// Boundary for executing a prepared image-generation request.
pub trait ImageGenerationRuntimeClient {
    /// Generates one image file for one prepared request.
    fn generate_image(
        &'_ self,
        request: ImageGenerationRuntimeRequest,
    ) -> ImageGenerationPortFuture<'_, ImageGenerationResponse>;
}
