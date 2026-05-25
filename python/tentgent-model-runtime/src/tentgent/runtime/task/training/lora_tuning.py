from __future__ import annotations

from dataclasses import dataclass, replace

from tentgent.runtime.backends.lora_tuning import (
    LoraTuningBackendKind,
    LoraTuningBackendModel,
    LoraTuningRequest,
    LoraTuningResult,
)
from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.task import TaskKind

from ..inference.inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class LoraTuningTaskRequest:
    backend: LoraTuningBackendKind
    model: ModelRecord
    tuning: LoraTuningRequest


class LoraTuningTask(InferenceTask[LoraTuningTaskRequest, LoraTuningResult]):
    def __init__(
        self,
        *,
        task_ref: str,
        request: LoraTuningTaskRequest,
        resources: ResourceManager[LoraTuningBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.LORA_TUNING,
            request=request,
        )
        self._resources = resources

    def execute(self) -> LoraTuningResult:
        events: list[dict[str, object]] = []
        with self._resources.lease_model(
            self.request.backend,
            self.request.model,
            idle_timeout_seconds=0.0,
        ) as runner:
            result = runner.run_lora_tuning(
                self.request.tuning,
                emit=events.append,
            )
        return replace(result, events=tuple(events))
