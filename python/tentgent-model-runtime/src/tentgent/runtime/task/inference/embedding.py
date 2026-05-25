from __future__ import annotations

from dataclasses import dataclass

from tentgent.runtime.backends.embedding import (
    EmbeddingBackendModel,
    EmbeddingModelKind,
    EmbeddingRequest,
    EmbeddingResult,
)
from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.task import TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class EmbeddingInferenceRequest:
    model_kind: EmbeddingModelKind
    model: ModelRecord
    embedding: EmbeddingRequest


class EmbeddingTask(InferenceTask[EmbeddingInferenceRequest, EmbeddingResult]):
    def __init__(
        self,
        *,
        task_ref: str,
        request: EmbeddingInferenceRequest,
        resources: ResourceManager[EmbeddingBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.EMBEDDING,
            request=request,
        )
        self._resources = resources

    def execute(self) -> EmbeddingResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as embedding_model:
            return embedding_model.embed(self.request.embedding)
