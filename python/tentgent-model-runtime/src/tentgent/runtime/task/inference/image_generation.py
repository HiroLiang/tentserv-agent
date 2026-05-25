from __future__ import annotations

from dataclasses import dataclass

from tentgent.runtime.backends.image_generation import (
    ImageGenerationAdapterSelection,
    ImageGenerationBackendModel,
    ImageGenerationModelKind,
    ImageGenerationRequest,
    ImageGenerationResult,
)
from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.task import TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class ImageGenerationInferenceRequest:
    model_kind: ImageGenerationModelKind
    model: ModelRecord
    image: ImageGenerationRequest


class ImageGenerationTask(
    InferenceTask[ImageGenerationInferenceRequest, ImageGenerationResult],
):
    def __init__(
        self,
        *,
        task_ref: str,
        request: ImageGenerationInferenceRequest,
        resources: ResourceManager[ImageGenerationBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.IMAGE_GENERATION,
            request=request,
        )
        self._resources = resources

    def execute(self) -> ImageGenerationResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as image_model:
            _select_adapter(image_model, self.request.image.adapter)
            return image_model.generate_image(self.request.image)


def _select_adapter(
    model: ImageGenerationBackendModel,
    adapter: ImageGenerationAdapterSelection | None,
) -> None:
    select_adapter = getattr(model, "select_adapter", None)
    if callable(select_adapter):
        select_adapter(adapter)
    elif adapter is not None:
        raise RuntimeError(
            f"model `{model.__class__.__name__}` does not support image adapters"
        )
