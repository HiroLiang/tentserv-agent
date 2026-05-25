from __future__ import annotations

from dataclasses import dataclass

from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.backends.video_understanding import (
    VideoUnderstandingBackendModel,
    VideoUnderstandingModelKind,
    VideoUnderstandingRequest,
    VideoUnderstandingResult,
)
from tentgent.runtime.task.task import TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class VideoUnderstandingInferenceRequest:
    model_kind: VideoUnderstandingModelKind
    model: ModelRecord
    video: VideoUnderstandingRequest


class VideoUnderstandingTask(
    InferenceTask[VideoUnderstandingInferenceRequest, VideoUnderstandingResult],
):
    def __init__(
        self,
        *,
        task_ref: str,
        request: VideoUnderstandingInferenceRequest,
        resources: ResourceManager[VideoUnderstandingBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.VIDEO_UNDERSTANDING,
            request=request,
        )
        self._resources = resources

    def execute(self) -> VideoUnderstandingResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as video_model:
            return video_model.understand_video(self.request.video)
