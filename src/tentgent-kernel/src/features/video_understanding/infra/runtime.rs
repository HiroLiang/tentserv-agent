use serde::{Deserialize, Serialize};

use crate::features::runtime::infra::{ModelRuntimeCapability, ModelRuntimeDaemonSupervisor};
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    VideoSamplingOptions, VideoUnderstandingOutputFormat, VideoUnderstandingRequest,
    VideoUnderstandingResponse, VideoUnderstandingRuntimeTarget,
};
use super::super::ports::{
    VideoUnderstandingPortFuture, VideoUnderstandingRuntimeClient, VideoUnderstandingRuntimeRequest,
};

/// Executes prepared video-understanding requests through the model-runtime HTTP daemon.
pub struct PythonVideoUnderstandingModelRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
    supervisor: &'a ModelRuntimeDaemonSupervisor,
}

impl<'a> PythonVideoUnderstandingModelRuntimeClient<'a> {
    pub fn new(
        executable_resolver: &'a dyn RuntimeExecutableResolver,
        supervisor: &'a ModelRuntimeDaemonSupervisor,
    ) -> Self {
        Self {
            executable_resolver,
            supervisor,
        }
    }

    async fn understand_http(
        &self,
        request: VideoUnderstandingRuntimeRequest,
    ) -> KernelResult<VideoUnderstandingResponse> {
        let model_ref = local_model_ref(&request.request);
        let endpoint = self
            .supervisor
            .ensure_model_bound(
                &request.layout,
                &request.runtime,
                self.executable_resolver,
                ModelRuntimeCapability::VideoUnderstanding,
                model_ref,
            )
            .await?;
        let payload = VideoUnderstandingPayload {
            video_path: request.request.video_path.display().to_string(),
            prompt: request.request.prompt.prompt,
            output_format: request.request.output_format,
            system_prompt: request.request.prompt.system_prompt,
            max_tokens: request.request.options.max_tokens,
            temperature: request.request.options.temperature,
            sampling: Some(VideoSamplingPayload::from(request.request.sampling)),
        };
        let response: VideoUnderstandingResponsePayload = self
            .supervisor
            .post_json(
                &endpoint,
                "/v1/video/understanding",
                &payload,
                video_runtime_error,
            )
            .await?;
        Ok(VideoUnderstandingResponse {
            output_format: response.output_format,
            media_type: response.media_type,
            text: response.text,
            finish_reason: response.finish_reason,
            sampled_frames: response.sampled_frames,
        })
    }
}

impl VideoUnderstandingRuntimeClient for PythonVideoUnderstandingModelRuntimeClient<'_> {
    fn understand_video(
        &'_ self,
        request: VideoUnderstandingRuntimeRequest,
    ) -> VideoUnderstandingPortFuture<'_, VideoUnderstandingResponse> {
        Box::pin(async move { self.understand_http(request).await })
    }
}

#[derive(Debug, Serialize)]
struct VideoUnderstandingPayload {
    video_path: String,
    prompt: String,
    output_format: VideoUnderstandingOutputFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sampling: Option<VideoSamplingPayload>,
}

#[derive(Debug, Serialize)]
struct VideoSamplingPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_fps: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_frame_edge: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clip_start_seconds: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clip_duration_seconds: Option<f32>,
}

impl From<VideoSamplingOptions> for VideoSamplingPayload {
    fn from(value: VideoSamplingOptions) -> Self {
        Self {
            sample_fps: value.sample_fps,
            max_frames: value.max_frames,
            max_frame_edge: value.max_frame_edge,
            clip_start_seconds: value.clip_start_seconds,
            clip_duration_seconds: value.clip_duration_seconds,
        }
    }
}

#[derive(Debug, Deserialize)]
struct VideoUnderstandingResponsePayload {
    output_format: VideoUnderstandingOutputFormat,
    media_type: String,
    text: String,
    finish_reason: String,
    sampled_frames: Option<u32>,
}

fn local_model_ref(request: &VideoUnderstandingRequest) -> &str {
    match &request.target.runtime {
        VideoUnderstandingRuntimeTarget::LocalModel { model_ref, .. } => model_ref.as_str(),
    }
}

fn video_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::VideoRuntimeUnavailable(message.into())
}
