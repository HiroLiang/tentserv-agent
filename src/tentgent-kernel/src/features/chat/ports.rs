//! Chat feature package ports.

use std::{future::Future, pin::Pin};

use crate::features::adapter::domain::{
    AdapterCompatibilityTarget, AdapterInspection, AdapterRefSelector,
};
use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::PythonRuntimeLayout;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

use super::domain::{
    ChatRequest, ChatResponse, ChatRuntimeTarget, ChatStreamEvent, ResolvedChatAdapter,
};

pub type ChatPortFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatModelResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: ModelRefSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatModelResolveResult {
    pub layout: RuntimeLayout,
    pub model: ModelInspection,
    pub target: ChatRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAdapterResolveRequest {
    pub layout: RuntimeLayoutInput,
    pub selector: AdapterRefSelector,
    pub target: AdapterCompatibilityTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAdapterResolveResult {
    pub layout: RuntimeLayout,
    pub adapter: AdapterInspection,
    pub target: ResolvedChatAdapter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRuntimeRequest {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub request: ChatRequest,
}

/// Boundary for resolving a model selector into a chat-capable runtime target.
pub trait ChatModelResolver {
    /// Resolves a model ref or unique prefix and maps it to a chat runtime target.
    fn resolve_chat_model(
        &self,
        request: ChatModelResolveRequest,
    ) -> KernelResult<ChatModelResolveResult>;
}

/// Boundary for resolving and validating an adapter for a selected chat target.
pub trait ChatAdapterResolver {
    /// Resolves an adapter ref or unique prefix and validates backend/model compatibility.
    fn resolve_chat_adapter(
        &self,
        request: ChatAdapterResolveRequest,
    ) -> KernelResult<ChatAdapterResolveResult>;
}

/// Boundary for executing a prepared chat request against the selected runtime.
pub trait ChatRuntimeClient {
    /// Generates a complete response for one prepared chat request.
    fn generate_chat<'a>(&'a self, request: ChatRuntimeRequest)
        -> ChatPortFuture<'a, ChatResponse>;

    /// Streams a prepared chat request and returns the terminal response metadata.
    fn stream_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatPortFuture<'a, ChatResponse>;
}
