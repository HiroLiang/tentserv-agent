from __future__ import annotations

from concurrent.futures import Future, ThreadPoolExecutor
from dataclasses import dataclass
from enum import StrEnum
from threading import RLock
from time import monotonic
from typing import Generic, TypeVar

from .task import RuntimeTask, TaskCanceled, TaskStatus


ResultT = TypeVar("ResultT")


class TaskManagerState(StrEnum):
    OPEN = "open"
    CLOSING = "closing"
    SHUTDOWN = "shutdown"


class TaskManagerClosedError(RuntimeError):
    pass


@dataclass(frozen=True, slots=True)
class TaskHandle(Generic[ResultT]):
    task_ref: str
    task: RuntimeTask[object, ResultT]
    future: Future[ResultT]


@dataclass(slots=True)
class _TrackedTask:
    task: RuntimeTask[object, object]
    future: Future[object]
    terminal_observed_at: float | None = None


class TaskManager:
    def __init__(
        self,
        *,
        max_workers: int = 4,
        completed_retention_seconds: float = 1.0,
    ) -> None:
        self._lock = RLock()
        self._executor = ThreadPoolExecutor(
            max_workers=max_workers,
            thread_name_prefix="tentgent-runtime-task",
        )
        self._tasks: dict[str, _TrackedTask] = {}
        self._state = TaskManagerState.OPEN
        self._last_activity_at = monotonic()
        self._completed_retention_seconds = completed_retention_seconds

    @property
    def state(self) -> TaskManagerState:
        with self._lock:
            return self._state

    def submit(
        self,
        task: RuntimeTask[object, ResultT],
    ) -> TaskHandle[ResultT]:
        with self._lock:
            if self._state != TaskManagerState.OPEN:
                raise TaskManagerClosedError(
                    f"task manager is {self._state}; rejecting task `{task.task_ref}`"
                )
            if task.task_ref in self._tasks:
                raise ValueError(f"task `{task.task_ref}` already exists")

            future = self._executor.submit(self._run_task, task)
            self._tasks[task.task_ref] = _TrackedTask(
                task=task,
                future=future,
            )
            self._last_activity_at = monotonic()

        return TaskHandle(
            task_ref=task.task_ref,
            task=task,
            future=future,
        )

    def poll_completed(self) -> None:
        now = monotonic()
        with self._lock:
            for task_ref, tracked in list(self._tasks.items()):
                if not tracked.future.done() or not tracked.task.is_terminal:
                    continue
                if tracked.terminal_observed_at is None:
                    tracked.terminal_observed_at = now
                    continue
                if (
                    now - tracked.terminal_observed_at
                    >= self._completed_retention_seconds
                ):
                    self._tasks.pop(task_ref, None)

    def begin_closing(self) -> None:
        with self._lock:
            if self._state == TaskManagerState.OPEN:
                self._state = TaskManagerState.CLOSING

    def mark_shutdown(self) -> None:
        with self._lock:
            self._state = TaskManagerState.SHUTDOWN

    def has_active_tasks(self) -> bool:
        with self._lock:
            return any(
                not tracked.future.done() or not tracked.task.is_terminal
                for tracked in self._tasks.values()
            )

    def is_idle_for(self, seconds: float) -> bool:
        with self._lock:
            if self._tasks:
                return False
            return monotonic() - self._last_activity_at >= seconds

    def snapshot(self) -> dict[str, object]:
        with self._lock:
            statuses: dict[str, int] = {status.value: 0 for status in TaskStatus}
            for tracked in self._tasks.values():
                statuses[tracked.task.status.value] += 1
            return {
                "state": self._state.value,
                "task_count": len(self._tasks),
                "tasks_by_status": statuses,
                "last_activity_age_seconds": round(
                    monotonic() - self._last_activity_at,
                    3,
                ),
            }

    def shutdown(self) -> None:
        self.mark_shutdown()
        self._executor.shutdown(wait=False, cancel_futures=True)

    def _run_task(self, task: RuntimeTask[object, ResultT]) -> ResultT:
        task.mark_running()
        try:
            result = task.execute()
            if task.cancel_requested:
                task.mark_canceled()
                raise TaskCanceled(f"task `{task.task_ref}` was canceled")
            task.mark_done(result)
            return result
        except TaskCanceled:
            task.mark_canceled()
            raise
        except BaseException as exc:
            task.mark_failed(exc)
            raise
        finally:
            with self._lock:
                self._last_activity_at = monotonic()
