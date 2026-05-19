//! Vision feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{VisionChatRequest, VisionChatResponse, VisionChatRuntimeTarget};

pub type VisionPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionChatModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionChatModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: VisionChatRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisionChatRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: VisionChatRequest,
}

/// Boundary for resolving a model selector into a vision-chat runtime target.
pub trait VisionChatModelResolver {
    /// Resolves a model ref or unique prefix and maps it to a vision-chat target.
    fn resolve_vision_chat_model(
        &self,
        request: VisionChatModelResolveRequest,
    ) -> KernelResult<VisionChatModelResolveResult>;
}

/// Boundary for executing a prepared vision-chat request.
pub trait VisionChatRuntimeClient {
    /// Generates a complete image-plus-text response for one prepared request.
    fn generate_vision_chat(
        &'_ self,
        request: VisionChatRuntimeRequest,
    ) -> VisionPortFuture<'_, VisionChatResponse>;
}
