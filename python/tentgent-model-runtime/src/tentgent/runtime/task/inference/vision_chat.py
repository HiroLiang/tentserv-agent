from __future__ import annotations

from dataclasses import dataclass

from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.backends.vision_chat import (
    VisionChatBackendModel,
    VisionChatModelKind,
    VisionChatRequest,
    VisionChatResult,
)
from tentgent.runtime.task.task import TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class VisionChatInferenceRequest:
    model_kind: VisionChatModelKind
    model: ModelRecord
    vision: VisionChatRequest


class VisionChatTask(InferenceTask[VisionChatInferenceRequest, VisionChatResult]):
    def __init__(
        self,
        *,
        task_ref: str,
        request: VisionChatInferenceRequest,
        resources: ResourceManager[VisionChatBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.VISION_CHAT,
            request=request,
        )
        self._resources = resources

    def execute(self) -> VisionChatResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as vision_model:
            return vision_model.generate_vision_chat(self.request.vision)
