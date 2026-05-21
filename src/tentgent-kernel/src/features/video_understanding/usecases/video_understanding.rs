//! Standard video-understanding use case orchestration.

use crate::features::runtime::usecases::{RuntimeResolutionRequest, RuntimeResolutionUseCase};
use crate::features::video_understanding::domain::{
    ResolvedVideoUnderstandingTarget, VideoUnderstandingPrompt, VideoUnderstandingRequest,
};
use crate::features::video_understanding::ports::{
    VideoUnderstandingModelResolveRequest, VideoUnderstandingModelResolver,
    VideoUnderstandingRuntimeClient, VideoUnderstandingRuntimeRequest,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutInput;

use super::port::{
    VideoUnderstandingExecutionResult, VideoUnderstandingPreparationRequest,
    VideoUnderstandingPreparationResult, VideoUnderstandingPreparationUseCase,
    VideoUnderstandingUseCase, VideoUnderstandingUseCaseFuture,
};

pub struct StdVideoUnderstandingUseCase<'a> {
    runtime_resolution: &'a dyn RuntimeResolutionUseCase,
    model_resolver: &'a dyn VideoUnderstandingModelResolver,
    runtime_client: &'a dyn VideoUnderstandingRuntimeClient,
}

impl<'a> StdVideoUnderstandingUseCase<'a> {
    pub fn new(
        runtime_resolution: &'a dyn RuntimeResolutionUseCase,
        model_resolver: &'a dyn VideoUnderstandingModelResolver,
        runtime_client: &'a dyn VideoUnderstandingRuntimeClient,
    ) -> Self {
        Self {
            runtime_resolution,
            model_resolver,
            runtime_client,
        }
    }
}

impl VideoUnderstandingPreparationUseCase for StdVideoUnderstandingUseCase<'_> {
    fn prepare_video_understanding(
        &self,
        request: VideoUnderstandingPreparationRequest,
    ) -> KernelResult<VideoUnderstandingPreparationResult> {
        request
            .sampling
            .validate()
            .map_err(|error| KernelError::UnsupportedTarget(error.to_string()))?;
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
        let model = self.model_resolver.resolve_video_understanding_model(
            VideoUnderstandingModelResolveRequest {
                layout: resolved_layout_input,
                selector: request.model_selector,
            },
        )?;
        let target = ResolvedVideoUnderstandingTarget {
            runtime: model.target.clone(),
        };
        let prompt = VideoUnderstandingPrompt::new(request.prompt, request.system_prompt)
            .map_err(|error| KernelError::UnsupportedTarget(error.to_string()))?;

        Ok(VideoUnderstandingPreparationResult {
            layout: runtime.layout,
            runtime: runtime.runtime,
            model: model.model,
            request: VideoUnderstandingRequest {
                target,
                video_path: request.video_path,
                video_media_type: request.video_media_type,
                prompt,
                output_format: request.output_format,
                options: request.options,
                sampling: request.sampling,
            },
        })
    }
}

impl VideoUnderstandingUseCase for StdVideoUnderstandingUseCase<'_> {
    fn understand_video(
        &'_ self,
        request: VideoUnderstandingPreparationRequest,
    ) -> VideoUnderstandingUseCaseFuture<'_, VideoUnderstandingExecutionResult> {
        Box::pin(async move {
            let prepared = self.prepare_video_understanding(request)?;
            let response = self
                .runtime_client
                .understand_video(VideoUnderstandingRuntimeRequest {
                    layout: prepared.layout.clone(),
                    runtime: prepared.runtime.clone(),
                    request: prepared.request.clone(),
                })
                .await?;

            Ok(VideoUnderstandingExecutionResult { prepared, response })
        })
    }
}
