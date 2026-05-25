from __future__ import annotations

import asyncio
import os
from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime
from enum import StrEnum
from time import monotonic
from typing import Any

from tentgent.runtime import __version__
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.manager import TaskManager, TaskManagerState


class RuntimeCapability(StrEnum):
    CHAT = "chat"
    EMBEDDING = "embedding"
    RERANK = "rerank"


@dataclass(frozen=True)
class RuntimeServerConfig:
    host: str
    port: int
    capability: RuntimeCapability = RuntimeCapability.CHAT
    server_ref: str | None = None
    model_ref: str | None = None
    idle_keep_alive_seconds: float = 300.0
    model_idle_timeout_seconds: float = 0.0
    closing_grace_seconds: float = 2.0
    task_poll_interval_seconds: float = 0.5


class RuntimeLifecycleState:
    def __init__(
        self,
        *,
        config: RuntimeServerConfig,
        task_manager: TaskManager,
        resource_manager: ResourceManager[Any],
        request_shutdown: Callable[[], None] | None = None,
    ) -> None:
        self._config = config
        self._task_manager = task_manager
        self._resource_manager = resource_manager
        self._request_shutdown = request_shutdown
        self._pid = os.getpid()
        self._started_at = datetime.now(UTC)
        self._started_monotonic = monotonic()
        self._watcher: asyncio.Task[None] | None = None
        self._closing_started_at: float | None = None

    async def start(self) -> None:
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
                "resources": self._resource_manager.snapshot(),
            },
            "tasks": task_snapshot,
        }

    async def _watch_idle(self) -> None:
        while True:
            await asyncio.sleep(self._config.task_poll_interval_seconds)
            self._task_manager.poll_completed()
            self._resource_manager.release_idle()

            if self._task_manager.state == TaskManagerState.OPEN:
                if self._task_manager.is_idle_for(
                    self._config.idle_keep_alive_seconds
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

    @staticmethod
    def _status_for_task_state(state: TaskManagerState) -> str:
        if state == TaskManagerState.OPEN:
            return "ok"
        if state == TaskManagerState.CLOSING:
            return "closing"
        return "shutdown"
