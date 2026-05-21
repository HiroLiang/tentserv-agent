//! Standard chat use case orchestration.

use crate::features::adapter::domain::AdapterCompatibilityTarget;
use crate::features::chat::domain::{
    ChatRequest, ChatRuntimeTarget, ChatStreamEvent, ResolvedChatTarget,
};
use crate::features::chat::ports::{
    ChatAdapterResolveRequest, ChatAdapterResolver, ChatModelResolveRequest, ChatModelResolver,
    ChatRuntimeClient, ChatRuntimeRequest,
};
use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    ChatCompletionResult, ChatCompletionUseCase, ChatPreparationRequest, ChatPreparationResult,
    ChatPreparationUseCase, ChatStreamingUseCase, ChatTargetSelection, ChatUseCaseFuture,
};

/// Standard orchestration for preparing and executing chat requests.
pub struct StdChatUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn ChatModelResolver,
    adapter_resolver: &'a dyn ChatAdapterResolver,
    runtime_client: &'a dyn ChatRuntimeClient,
}

impl<'a> StdChatUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn ChatModelResolver,
        adapter_resolver: &'a dyn ChatAdapterResolver,
        runtime_client: &'a dyn ChatRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            adapter_resolver,
            runtime_client,
        }
    }
}

impl ChatPreparationUseCase for StdChatUseCase<'_> {
    fn prepare_chat(&self, request: ChatPreparationRequest) -> KernelResult<ChatPreparationResult> {
        let mode = request.layout.mode;
        let runtime = self
            .runtime_resolution
            .resolve_runtime(RuntimeResolutionRequest {
                layout: request.layout,
                runtime: request.runtime,
            })?;
        let resolved_layout_input = RuntimeLayoutInput {
            mode,
            home_dir: Some(runtime.layout.home_dir.clone()),
            data_root_dir: Some(runtime.layout.data_root_dir.clone()),
        };

        let (model, adapter, target) = match request.target {
            ChatTargetSelection::LocalModel {
                model_selector,
                adapter_selector,
            } => {
                let model = self
                    .model_resolver
                    .resolve_chat_model(ChatModelResolveRequest {
                        layout: resolved_layout_input.clone(),
                        selector: model_selector,
                    })?;
                let adapter = match (adapter_selector, &model.target) {
                    (
                        Some(adapter_selector),
                        ChatRuntimeTarget::LocalModel {
                            model_ref,
                            backend,
                            source_repo,
                            source_revision,
                            model_capabilities,
                        },
                    ) => Some(self.adapter_resolver.resolve_chat_adapter(
                        ChatAdapterResolveRequest {
                            layout: resolved_layout_input,
                            selector: adapter_selector,
                            target: AdapterCompatibilityTarget {
                                base_model_ref: model_ref.clone(),
                                base_model_source_repo: source_repo.clone(),
                                base_model_source_revision: source_revision.clone(),
                                base_model_capabilities: model_capabilities.clone(),
                                required_capability:
                                    crate::features::model::domain::ModelCapability::Chat,
                                backend: backend.adapter_backend_support(),
                            },
                        },
                    )?),
                    (None, _) => None,
                    (Some(_), ChatRuntimeTarget::CloudProvider { .. }) => {
                        return Err(KernelError::UnsupportedTarget(
                            "cloud chat targets do not support adapters".to_string(),
                        ))
                    }
                };
                let target = ResolvedChatTarget {
                    runtime: model.target.clone(),
                    adapter: adapter.as_ref().map(|adapter| adapter.target.clone()),
                };

                (
                    Some(model.model),
                    adapter.map(|adapter| adapter.adapter),
                    target,
                )
            }
            ChatTargetSelection::CloudProvider {
                provider,
                provider_model,
            } => {
                let provider_model = provider_model.trim().to_string();
                if provider_model.is_empty() {
                    return Err(KernelError::UnsupportedTarget(
                        "cloud provider model must not be empty".to_string(),
                    ));
                }
                (
                    None,
                    None,
                    ResolvedChatTarget {
                        runtime: ChatRuntimeTarget::CloudProvider {
                            provider,
                            provider_model,
                        },
                        adapter: None,
                    },
                )
            }
        };

        Ok(ChatPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model,
            adapter,
            request: ChatRequest {
                target,
                prompt: request.prompt,
                options: request.options,
            },
        })
    }
}

impl ChatCompletionUseCase for StdChatUseCase<'_> {
    fn complete_chat(
        &'_ self,
        request: ChatPreparationRequest,
    ) -> ChatUseCaseFuture<'_, ChatCompletionResult> {
        Box::pin(async move {
            let prepared = self.prepare_chat(request)?;
            let response = self
                .runtime_client
                .generate_chat(ChatRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await?;

            Ok(ChatCompletionResult { prepared, response })
        })
    }
}

impl ChatStreamingUseCase for StdChatUseCase<'_> {
    fn stream_chat<'a>(
        &'a self,
        request: ChatPreparationRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatUseCaseFuture<'a, ChatCompletionResult> {
        Box::pin(async move {
            let prepared = self.prepare_chat(request)?;
            let response = self
                .runtime_client
                .stream_chat(
                    ChatRuntimeRequest {
                        layout: prepared.layout.clone(),
                        runtime: prepared.runtime.clone(),
                        request: prepared.request.clone(),
                    },
                    sink,
                )
                .await?;

            Ok(ChatCompletionResult { prepared, response })
        })
    }
}
