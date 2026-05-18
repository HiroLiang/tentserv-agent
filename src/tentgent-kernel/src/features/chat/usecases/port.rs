//! Chat use case ports.

use std::{future::Future, pin::Pin};

use crate::features::adapter::domain::{AdapterInspection, AdapterRefSelector};
use crate::features::auth::domain::Provider;
use crate::features::chat::domain::{
    ChatGenerationOptions, ChatPrompt, ChatRequest, ChatResponse, ChatStreamEvent,
};
use crate::features::model::domain::{ModelInspection, ModelRefSelector};
use crate::features::runtime::domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{RuntimeLayout, RuntimeLayoutInput};

/// Boxed async return type used by chat use cases that execute runtime work.
pub type ChatUseCaseFuture<'a, T> = Pin<Box<dyn Future<Output = KernelResult<T>> + 'a>>;

/// User-selected chat target before model, adapter, and runtime resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatTargetSelection {
    LocalModel {
        model_selector: ModelRefSelector,
        adapter_selector: Option<AdapterRefSelector>,
    },
    CloudProvider {
        provider: Provider,
        provider_model: String,
    },
}

/// Request for preparing one chat turn without executing generation.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatPreparationRequest {
    pub layout: RuntimeLayoutInput,
    pub runtime: PythonRuntimeResolutionInput,
    pub target: ChatTargetSelection,
    pub prompt: ChatPrompt,
    pub options: ChatGenerationOptions,
}

/// Result of resolving layout, runtime, model, adapter, and the runtime request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatPreparationResult {
    pub layout: RuntimeLayout,
    pub runtime: PythonRuntimeLayout,
    pub model: Option<ModelInspection>,
    pub adapter: Option<AdapterInspection>,
    pub request: ChatRequest,
}

/// Result of executing one prepared chat request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatCompletionResult {
    pub prepared: ChatPreparationResult,
    pub response: ChatResponse,
}

/// Use-case boundary for preparing chat runtime requests.
pub trait ChatPreparationUseCase {
    /// Resolves the selected target and builds the canonical runtime request.
    fn prepare_chat(&self, request: ChatPreparationRequest) -> KernelResult<ChatPreparationResult>;
}

/// Use-case boundary for one-shot non-streaming chat generation.
pub trait ChatCompletionUseCase {
    /// Resolves target/runtime and returns the complete generated response.
    fn complete_chat(
        &'_ self,
        request: ChatPreparationRequest,
    ) -> ChatUseCaseFuture<'_, ChatCompletionResult>;
}

/// Use-case boundary for streaming chat generation.
pub trait ChatStreamingUseCase {
    /// Resolves target/runtime, sends delta events to the sink, and returns terminal metadata.
    fn stream_chat<'a>(
        &'a self,
        request: ChatPreparationRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatUseCaseFuture<'a, ChatCompletionResult>;
}
