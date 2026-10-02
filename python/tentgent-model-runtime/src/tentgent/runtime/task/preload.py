from __future__ import annotations

from typing import Any

from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager

from .task import RuntimeTask, TaskKind


class PreloadTask(RuntimeTask[object, None]):
    """Validate a bound base model through the ordinary resource lease path."""

    def __init__(
        self,
        *,
        task_ref: str,
        model_kind: str,
        model: ModelRecord,
        resource_manager: ResourceManager[Any],
    ) -> None:
        super().__init__(task_ref=task_ref, kind=TaskKind.PRELOAD, request=model)
        self._model_kind = model_kind
        self._model = model
        self._resource_manager = resource_manager

    def execute(self) -> None:
        with self._resource_manager.lease_model(self._model_kind, self._model):
            pass
        # Completion includes lease exit (and immediate release for idle 0).
