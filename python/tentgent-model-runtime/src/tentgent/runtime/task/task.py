from __future__ import annotations

from abc import ABC, abstractmethod
from threading import Event
from enum import StrEnum
from time import monotonic
from typing import Generic, TypeVar

RequestT = TypeVar("RequestT")
ResultT = TypeVar("ResultT")


class TaskKind(StrEnum):
    AUDIO_SPEECH = "audio-speech"
    AUDIO_TRANSCRIPTION = "audio-transcription"
    CHAT = "chat"
    CHAT_STREAM = "chat-stream"
    EMBEDDING = "embedding"
    IMAGE_GENERATION = "image-generation"
    RERANK = "rerank"
    VIDEO_UNDERSTANDING = "video-understanding"
    VISION_CHAT = "vision-chat"


class TaskStatus(StrEnum):
    PENDING = "pending"
    RUNNING = "running"
    DONE = "done"
    FAILED = "failed"
    CANCELED = "canceled"


class ExecutionTarget(StrEnum):
    AUTO = "auto"
    LOCAL = "local"
    CLOUD = "cloud"


class TaskCanceled(RuntimeError):
    pass


class RuntimeTask(ABC, Generic[RequestT, ResultT]):
    def __init__(
        self,
        *,
        task_ref: str,
        kind: TaskKind,
        request: RequestT,
        execution_target: ExecutionTarget = ExecutionTarget.LOCAL,
    ) -> None:
        self.task_ref = task_ref
        self.kind = kind
        self.request = request
        self.execution_target = execution_target
        self.status = TaskStatus.PENDING
        self.result: ResultT | None = None
        self.error: BaseException | None = None
        self.created_at = monotonic()
        self.started_at: float | None = None
        self.completed_at: float | None = None
        self._cancel_requested = Event()

    @abstractmethod
    def execute(self) -> ResultT:
        raise NotImplementedError

    def mark_running(self) -> None:
        self.status = TaskStatus.RUNNING
        self.started_at = monotonic()

    def mark_done(self, result: ResultT) -> None:
        self.result = result
        self.status = TaskStatus.DONE
        self.completed_at = monotonic()

    def mark_failed(self, error: BaseException) -> None:
        self.error = error
        self.status = TaskStatus.FAILED
        self.completed_at = monotonic()

    def mark_canceled(self) -> None:
        self.status = TaskStatus.CANCELED
        self.completed_at = monotonic()

    def cancel(self) -> None:
        self._cancel_requested.set()

    @property
    def cancel_requested(self) -> bool:
        return self._cancel_requested.is_set()

    @property
    def is_terminal(self) -> bool:
        return self.status in {
            TaskStatus.DONE,
            TaskStatus.FAILED,
            TaskStatus.CANCELED,
        }

    async def close(self) -> None:
        """Optional cleanup hook"""
        return None
