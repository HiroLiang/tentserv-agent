from __future__ import annotations

from dataclasses import dataclass
from queue import Queue
from typing import Any

from tentgent.runtime.backends.chat import (
    ChatBackendModel,
    ChatModelKind,
    ChatRequest,
    ChatResult,
)
from tentgent.runtime.backends.records import AdapterRecord, ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.task import TaskCanceled, TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class ChatInferenceRequest:
    model_kind: ChatModelKind
    model: ModelRecord
    chat: ChatRequest
    adapter: AdapterRecord | None = None


@dataclass(frozen=True, slots=True)
class StreamEvent:
    event: str
    data: dict[str, Any]


class ChatTask(InferenceTask[ChatInferenceRequest, ChatResult]):
    def __init__(
        self,
        *,
        task_ref: str,
        request: ChatInferenceRequest,
        resources: ResourceManager[ChatBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.CHAT,
            request=request,
        )
        self._resources = resources

    def execute(self) -> ChatResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as chat_model:
            _select_adapter(chat_model, self.request.adapter)
            return chat_model.generate(self.request.chat)


class StreamingChatTask(InferenceTask[ChatInferenceRequest, ChatResult]):
    def __init__(
        self,
        *,
        task_ref: str,
        request: ChatInferenceRequest,
        resources: ResourceManager[ChatBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.CHAT_STREAM,
            request=request,
        )
        self._resources = resources
        self._events: Queue[StreamEvent | None] = Queue()

    def execute(self) -> ChatResult:
        chunks: list[str] = []
        self._put("started", {"task_ref": self.task_ref})
        try:
            with self._resources.lease_model(
                self.request.model_kind,
                self.request.model,
            ) as chat_model:
                _select_adapter(chat_model, self.request.adapter)
                for chunk in chat_model.stream_generate(self.request.chat):
                    if self.cancel_requested:
                        raise TaskCanceled(f"task `{self.task_ref}` was canceled")
                    chunks.append(chunk)
                    self._put("delta", {"text": chunk})

            result = ChatResult(text="".join(chunks))
            self._put("done", {"task_ref": self.task_ref, "text": result.text})
            return result
        except TaskCanceled:
            self._put("canceled", {"task_ref": self.task_ref})
            raise
        except BaseException as exc:
            self._put(
                "error",
                {
                    "task_ref": self.task_ref,
                    "type": exc.__class__.__name__,
                    "message": str(exc),
                },
            )
            raise
        finally:
            self._events.put(None)

    def iter_events(self):
        while True:
            event = self._events.get()
            if event is None:
                break
            yield event

    def _put(self, event: str, data: dict[str, Any]) -> None:
        self._events.put(StreamEvent(event=event, data=data))


def _select_adapter(model: ChatBackendModel, adapter: AdapterRecord | None) -> None:
    select_adapter = getattr(model, "select_adapter", None)
    if callable(select_adapter):
        select_adapter(adapter)
    elif adapter is not None:
        raise RuntimeError(
            f"model `{model.__class__.__name__}` does not support adapters"
        )
