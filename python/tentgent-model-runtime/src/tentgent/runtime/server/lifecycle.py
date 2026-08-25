from __future__ import annotations

import asyncio
import math
import os
from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path
from time import monotonic
from typing import TYPE_CHECKING, Any

from tentgent.runtime import __version__
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.manager import TaskManager, TaskManagerState

if TYPE_CHECKING:
    from .managed_models import BoundModelContext


class RuntimeCapability(StrEnum):
    AUDIO_SPEECH = "audio-speech"
    AUDIO_TRANSCRIPTION = "audio-transcription"
    CHAT = "chat"
    EMBEDDING = "embedding"
    IMAGE_GENERATION = "image-generation"
    LORA_TUNING = "lora-tuning"
    RERANK = "rerank"
    VIDEO_UNDERSTANDING = "video-understanding"
    VISION_CHAT = "vision-chat"


@dataclass(frozen=True)
class RuntimeServerConfig:
    host: str
    port: int
    capability: RuntimeCapability = RuntimeCapability.CHAT
    server_ref: str | None = None
    model_ref: str | None = None
    home: Path | None = None
    lazy_load: bool = True
    runtime_idle_seconds: float = 300.0
    model_idle_seconds: float = 0.0
    closing_grace_seconds: float = 2.0
    task_poll_interval_seconds: float = 0.5

    def __post_init__(self) -> None:
        for name, value in (
            ("runtime_idle_seconds", self.runtime_idle_seconds),
            ("model_idle_seconds", self.model_idle_seconds),
        ):
            if not math.isfinite(value) or value < 0:
                raise ValueError(f"{name} must be finite and non-negative")
        if self.model_idle_seconds > self.runtime_idle_seconds:
            raise ValueError(
                "model_idle_seconds must be less than or equal to "
                "runtime_idle_seconds"
            )


class RuntimeLifecycleState:
    def __init__(
        self,
        *,
        config: RuntimeServerConfig,
        task_manager: TaskManager,
        resource_manager: ResourceManager[Any],
        bound_model: BoundModelContext | None = None,
        request_shutdown: Callable[[], None] | None = None,
    ) -> None:
        self._config = config
        self._task_manager = task_manager
        self._resource_manager = resource_manager
        self._bound_model = bound_model
        self._request_shutdown = request_shutdown
        self._pid = os.getpid()
        self._process_token = os.environ.get("TENTGENT_RUNTIME_PROCESS_TOKEN")
        self._started_at = datetime.now(UTC)
        self._started_monotonic = monotonic()
        self._watcher: asyncio.Task[None] | None = None
        self._closing_started_at: float | None = None

    async def start(self) -> None:
        if self._config.model_ref is not None and not self._config.lazy_load:
            await asyncio.to_thread(self._preload_bound_model)
        # Runtime idleness begins once startup work has completed and the app is
        # ready to accept tasks. Preload time must not consume the idle budget.
        self._task_manager.touch_activity()
        self._watcher = asyncio.create_task(self._watch_idle())

    async def stop(self) -> None:
        if self._watcher is not None:
            self._watcher.cancel()
            try:
                await self._watcher
            except asyncio.CancelledError:
                pass
        self._resource_manager.release_all()
        self._task_manager.shutdown()

    def snapshot(self) -> dict[str, Any]:
        task_snapshot = self._task_manager.snapshot()
        status = self._status_for_task_state(self._task_manager.state)
        return {
            "status": status,
            "version": __version__,
            "pid": self._pid,
            "process_token": self._process_token,
            "server_ref": self._config.server_ref,
            "runtime_home": (
                str(self._config.home)
                if self._config.home is not None
                else os.environ.get("TENTGENT_HOME")
            ),
            "uptime_seconds": round(monotonic() - self._started_monotonic, 3),
            "started_at": self._started_at.isoformat(),
            "server": {
                "host": self._config.host,
                "port": self._config.port,
                "server_ref": self._config.server_ref,
            },
            "runtime": {
                "capability": self._config.capability.value,
                "model_ref": self._config.model_ref,
                "model_bound": self._bound_model is not None,
                "lifecycle": {
                    "runtime_idle_seconds": self._config.runtime_idle_seconds,
                    "model_idle_seconds": self._config.model_idle_seconds,
                },
                "resources": self._resource_manager.snapshot(),
            },
            "tasks": task_snapshot,
        }

    def begin_shutdown(self) -> dict[str, Any]:
        self._task_manager.begin_closing()
        if self._closing_started_at is None:
            self._closing_started_at = monotonic()
        return self.snapshot()

    async def _watch_idle(self) -> None:
        while True:
            await asyncio.sleep(self._config.task_poll_interval_seconds)
            self._task_manager.poll_completed()
            self._resource_manager.release_idle()

            if self._task_manager.state == TaskManagerState.OPEN:
                if self._task_manager.is_idle_for(
                    self._config.runtime_idle_seconds
                ):
                    self._task_manager.begin_closing()
                    self._closing_started_at = monotonic()
                continue

            if self._task_manager.state != TaskManagerState.CLOSING:
                continue

            if self._closing_started_at is None:
                self._closing_started_at = monotonic()

            closing_age = monotonic() - self._closing_started_at
            if (
                not self._task_manager.has_active_tasks()
                and closing_age >= self._config.closing_grace_seconds
            ):
                self._task_manager.mark_shutdown()
                if self._request_shutdown is not None:
                    self._request_shutdown()
                return

    def _preload_bound_model(self) -> None:
        from .managed_models import infer_preload_model_kind_for_capability

        if self._bound_model is None:
            return
        model_kind = infer_preload_model_kind_for_capability(
            self._config.capability,
            self._bound_model.model,
        )
        if model_kind is None:
            return
        with self._resource_manager.lease_model(model_kind, self._bound_model.model):
            pass

    @staticmethod
    def _status_for_task_state(state: TaskManagerState) -> str:
        if state == TaskManagerState.OPEN:
            return "ok"
        if state == TaskManagerState.CLOSING:
            return "closing"
        return "shutdown"
