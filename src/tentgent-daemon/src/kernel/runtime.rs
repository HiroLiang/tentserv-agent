use tentgent_kernel::{
    features::{
        audio::{
            domain::AudioTranscriptionResponse,
            infra::PythonAudioTranscriptionBatchRuntimeClient,
            ports::{
                AudioPortFuture, AudioTranscriptionRuntimeClient, AudioTranscriptionRuntimeRequest,
            },
        },
        chat::{
            domain::{ChatResponse, ChatStreamEvent},
            infra::PythonChatOnceRuntimeClient,
            ports::{ChatPortFuture, ChatRuntimeClient, ChatRuntimeRequest},
        },
        dataset::{
            domain::{DatasetRuntimeDebug, DatasetSynthRuntimeOutput},
            infra::{PythonDatasetEvalRuntimeClient, PythonDatasetSynthRuntimeClient},
            ports::{
                DatasetEvalRuntimeClient, DatasetEvalRuntimeRequest, DatasetPortFuture,
                DatasetSynthPromptRuntimeRequest, DatasetSynthRuntimeClient,
                DatasetSynthRuntimeRequest,
            },
        },
        embedding::{
            domain::EmbeddingResponse,
            infra::PythonEmbeddingOnceRuntimeClient,
            ports::{EmbeddingPortFuture, EmbeddingRuntimeClient, EmbeddingRuntimeRequest},
        },
        image_generation::{
            domain::ImageGenerationResponse,
            infra::PythonImageGenerationOnceRuntimeClient,
            ports::{
                ImageGenerationPortFuture, ImageGenerationRuntimeClient,
                ImageGenerationRuntimeRequest,
            },
        },
        rerank::{
            domain::RerankResponse,
            infra::PythonRerankOnceRuntimeClient,
            ports::{RerankPortFuture, RerankRuntimeClient, RerankRuntimeRequest},
        },
        runtime::{
            domain::{PythonRuntimeLayout, PythonRuntimeResolutionInput, RuntimeEntrypoint},
            infra::{
                StdPythonRuntimeResolver, StdRuntimeBootstrapExecutor, StdRuntimeBootstrapPlanner,
                StdRuntimeExecutableResolver, StdRuntimeStateProbe,
            },
            ports::{PythonRuntimeResolver, RuntimeExecutableResolver},
            usecases::{
                RuntimeBootstrapRequest, RuntimeBootstrapResult, RuntimeBootstrapUseCase,
                RuntimeExecutableResolutionRequest, RuntimeExecutableResolutionResult,
                RuntimeExecutableResolutionUseCase, RuntimeResolutionRequest,
                RuntimeResolutionResult, RuntimeResolutionUseCase, RuntimeStateRequest,
                RuntimeStateResult, RuntimeStateUseCase, StdRuntimeBootstrapUseCase,
                StdRuntimeExecutableResolutionUseCase, StdRuntimeResolutionUseCase,
                StdRuntimeStateUseCase,
            },
        },
        vision::{
            domain::VisionChatResponse,
            infra::PythonVisionChatOnceRuntimeClient,
            ports::{VisionChatRuntimeClient, VisionChatRuntimeRequest, VisionPortFuture},
        },
    },
    foundation::{
        error::KernelResult,
        layout::{RuntimeLayout, StdRuntimeLayoutResolver},
        platform::StdPlatformProbe,
    },
};

pub struct RuntimeKernelComponent {
    layout_resolver: StdRuntimeLayoutResolver,
    platform_probe: StdPlatformProbe,
    runtime_resolver: StdPythonRuntimeResolver,
    bootstrap_planner: StdRuntimeBootstrapPlanner,
    bootstrap_executor: StdRuntimeBootstrapExecutor,
    state_probe: StdRuntimeStateProbe,
    executable_resolver: StdRuntimeExecutableResolver,
}

impl RuntimeKernelComponent {
    pub fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            platform_probe: StdPlatformProbe,
            runtime_resolver: StdPythonRuntimeResolver,
            bootstrap_planner: StdRuntimeBootstrapPlanner,
            bootstrap_executor: StdRuntimeBootstrapExecutor,
            state_probe: StdRuntimeStateProbe,
            executable_resolver: StdRuntimeExecutableResolver,
        }
    }

    pub fn resolution_usecase(&self) -> StdRuntimeResolutionUseCase<'_> {
        StdRuntimeResolutionUseCase::new(&self.layout_resolver, &self.runtime_resolver)
    }

    pub fn bootstrap_usecase(&self) -> StdRuntimeBootstrapUseCase<'_> {
        StdRuntimeBootstrapUseCase::new(
            &self.layout_resolver,
            &self.platform_probe,
            &self.runtime_resolver,
            &self.bootstrap_planner,
            &self.bootstrap_executor,
        )
    }

    pub fn state_usecase(&self) -> StdRuntimeStateUseCase<'_> {
        StdRuntimeStateUseCase::new(
            &self.layout_resolver,
            &self.runtime_resolver,
            &self.state_probe,
        )
    }

    pub fn executable_resolution_usecase(&self) -> StdRuntimeExecutableResolutionUseCase<'_> {
        StdRuntimeExecutableResolutionUseCase::new(
            &self.layout_resolver,
            &self.runtime_resolver,
            &self.executable_resolver,
        )
    }
}

impl Default for RuntimeKernelComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonRuntimeResolver for RuntimeKernelComponent {
    fn resolve_python_runtime(
        &self,
        layout: &RuntimeLayout,
        input: PythonRuntimeResolutionInput,
    ) -> KernelResult<PythonRuntimeLayout> {
        self.runtime_resolver.resolve_python_runtime(layout, input)
    }
}

impl RuntimeExecutableResolver for RuntimeKernelComponent {
    fn python_binary_path(
        &self,
        runtime: &PythonRuntimeLayout,
    ) -> KernelResult<std::path::PathBuf> {
        self.executable_resolver.python_binary_path(runtime)
    }

    fn entrypoint_path(
        &self,
        runtime: &PythonRuntimeLayout,
        entrypoint: RuntimeEntrypoint,
    ) -> KernelResult<std::path::PathBuf> {
        self.executable_resolver
            .entrypoint_path(runtime, entrypoint)
    }
}

impl RuntimeResolutionUseCase for RuntimeKernelComponent {
    fn resolve_runtime(
        &self,
        request: RuntimeResolutionRequest,
    ) -> KernelResult<RuntimeResolutionResult> {
        self.resolution_usecase().resolve_runtime(request)
    }
}

impl RuntimeBootstrapUseCase for RuntimeKernelComponent {
    fn bootstrap_runtime(
        &self,
        request: RuntimeBootstrapRequest,
    ) -> KernelResult<RuntimeBootstrapResult> {
        self.bootstrap_usecase().bootstrap_runtime(request)
    }
}

impl RuntimeStateUseCase for RuntimeKernelComponent {
    fn runtime_state(&self, request: RuntimeStateRequest) -> KernelResult<RuntimeStateResult> {
        self.state_usecase().runtime_state(request)
    }
}

impl RuntimeExecutableResolutionUseCase for RuntimeKernelComponent {
    fn resolve_runtime_executable(
        &self,
        request: RuntimeExecutableResolutionRequest,
    ) -> KernelResult<RuntimeExecutableResolutionResult> {
        self.executable_resolution_usecase()
            .resolve_runtime_executable(request)
    }
}

impl ChatRuntimeClient for RuntimeKernelComponent {
    fn generate_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move {
            PythonChatOnceRuntimeClient::new(self)
                .generate_chat(request)
                .await
        })
    }

    fn stream_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move {
            PythonChatOnceRuntimeClient::new(self)
                .stream_chat(request, sink)
                .await
        })
    }
}

impl EmbeddingRuntimeClient for RuntimeKernelComponent {
    fn embed(
        &'_ self,
        request: EmbeddingRuntimeRequest,
    ) -> EmbeddingPortFuture<'_, EmbeddingResponse> {
        Box::pin(async move {
            PythonEmbeddingOnceRuntimeClient::new(self)
                .embed(request)
                .await
        })
    }
}

impl RerankRuntimeClient for RuntimeKernelComponent {
    fn rerank(&'_ self, request: RerankRuntimeRequest) -> RerankPortFuture<'_, RerankResponse> {
        Box::pin(async move {
            PythonRerankOnceRuntimeClient::new(self)
                .rerank(request)
                .await
        })
    }
}

impl AudioTranscriptionRuntimeClient for RuntimeKernelComponent {
    fn transcribe_audio(
        &'_ self,
        request: AudioTranscriptionRuntimeRequest,
    ) -> AudioPortFuture<'_, AudioTranscriptionResponse> {
        Box::pin(async move {
            PythonAudioTranscriptionBatchRuntimeClient::new(self)
                .transcribe_audio(request)
                .await
        })
    }
}

impl VisionChatRuntimeClient for RuntimeKernelComponent {
    fn generate_vision_chat(
        &'_ self,
        request: VisionChatRuntimeRequest,
    ) -> VisionPortFuture<'_, VisionChatResponse> {
        Box::pin(async move {
            PythonVisionChatOnceRuntimeClient::new(self)
                .generate_vision_chat(request)
                .await
        })
    }
}

impl ImageGenerationRuntimeClient for RuntimeKernelComponent {
    fn generate_image(
        &'_ self,
        request: ImageGenerationRuntimeRequest,
    ) -> ImageGenerationPortFuture<'_, ImageGenerationResponse> {
        Box::pin(async move {
            PythonImageGenerationOnceRuntimeClient::new(self)
                .generate_image(request)
                .await
        })
    }
}

impl DatasetSynthRuntimeClient for RuntimeKernelComponent {
    fn render_synth_prompt(
        &self,
        request: DatasetSynthPromptRuntimeRequest,
    ) -> DatasetPortFuture<'_, String> {
        Box::pin(async move {
            PythonDatasetSynthRuntimeClient::new(self)
                .render_synth_prompt(request)
                .await
        })
    }

    fn synthesize_dataset(
        &self,
        request: DatasetSynthRuntimeRequest,
    ) -> DatasetPortFuture<'_, DatasetSynthRuntimeOutput> {
        Box::pin(async move {
            PythonDatasetSynthRuntimeClient::new(self)
                .synthesize_dataset(request)
                .await
        })
    }
}

impl DatasetEvalRuntimeClient for RuntimeKernelComponent {
    fn evaluate_dataset(
        &self,
        request: DatasetEvalRuntimeRequest,
    ) -> DatasetPortFuture<'_, serde_json::Value> {
        Box::pin(async move {
            PythonDatasetEvalRuntimeClient::new(self)
                .evaluate_dataset(request)
                .await
        })
    }

    fn runtime_debug(&self, error_detail: &str) -> Option<DatasetRuntimeDebug> {
        PythonDatasetEvalRuntimeClient::new(self).runtime_debug(error_detail)
    }
}
