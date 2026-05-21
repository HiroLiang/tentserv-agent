//! Video-understanding feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{
    VideoUnderstandingRequest, VideoUnderstandingResponse, VideoUnderstandingRuntimeTarget,
};

pub type VideoUnderstandingPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoUnderstandingModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoUnderstandingModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: VideoUnderstandingRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoUnderstandingRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: VideoUnderstandingRequest,
}

pub trait VideoUnderstandingModelResolver {
    fn resolve_video_understanding_model(
        &self,
        request: VideoUnderstandingModelResolveRequest,
    ) -> KernelResult<VideoUnderstandingModelResolveResult>;
}

pub trait VideoUnderstandingRuntimeClient {
    fn understand_video(
        &'_ self,
        request: VideoUnderstandingRuntimeRequest,
    ) -> VideoUnderstandingPortFuture<'_, VideoUnderstandingResponse>;
}
