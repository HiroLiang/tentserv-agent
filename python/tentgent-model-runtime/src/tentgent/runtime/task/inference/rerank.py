from __future__ import annotations

from dataclasses import dataclass

from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.rerank import (
    RerankBackendModel,
    RerankModelKind,
    RerankRequest,
    RerankResult,
)
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.task import TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class RerankInferenceRequest:
    model_kind: RerankModelKind
    model: ModelRecord
    rerank: RerankRequest


class RerankTask(InferenceTask[RerankInferenceRequest, RerankResult]):
    def __init__(
        self,
        *,
        task_ref: str,
        request: RerankInferenceRequest,
        resources: ResourceManager[RerankBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.RERANK,
            request=request,
        )
        self._resources = resources

    def execute(self) -> RerankResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as rerank_model:
            return rerank_model.rerank(self.request.rerank)
