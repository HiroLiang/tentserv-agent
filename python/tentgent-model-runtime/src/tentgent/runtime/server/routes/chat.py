from __future__ import annotations

import asyncio
import json
from pathlib import Path
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, Field

from tentgent.runtime.backends.chat import (
    ChatMessage,
    ChatModelKind,
    ChatRequest,
)
from tentgent.runtime.backends.records import (
    AdapterRecord,
    AdapterType,
    ModelCapability,
    ModelFormat,
    ModelRecord,
)
from tentgent.runtime.task.inference.chat import (
    ChatInferenceRequest,
    ChatTask,
    StreamingChatTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError


router = APIRouter(prefix="/v1")


class ChatMessagePayload(BaseModel):
    role: str
    content: str


class ModelRecordPayload(BaseModel):
    model_ref: str
    source_path: str
    primary_format: ModelFormat
    capabilities: list[ModelCapability] = Field(default_factory=list)
    short_ref: str | None = None
    source_repo: str | None = None
    source_revision: str | None = None


class AdapterRecordPayload(BaseModel):
    adapter_ref: str
    source_path: str
    adapter_format: str
    adapter_type: AdapterType = AdapterType.LORA
    short_ref: str | None = None
    weight_file: str | None = None


class ChatPayload(BaseModel):
    task_ref: str | None = None
    model_kind: ChatModelKind
    model: ModelRecordPayload
    messages: list[ChatMessagePayload]
    max_tokens: int | None = None
    temperature: float | None = None
    adapter: AdapterRecordPayload | None = None


class ChatResponsePayload(BaseModel):
    task_ref: str
    status: str
    text: str


@router.post("/chat")
async def chat(payload: ChatPayload, request: Request) -> ChatResponsePayload:
    task = _build_chat_task(payload, request)
    task_manager = request.app.state.task_manager
    try:
        handle = task_manager.submit(task)
    except TaskManagerClosedError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    try:
        result = await asyncio.wrap_future(handle.future)
    except BaseException as exc:
        raise _http_exception(exc) from exc

    return ChatResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        text=result.text,
    )


@router.post("/chat/stream")
def stream_chat(payload: ChatPayload, request: Request) -> StreamingResponse:
    task = _build_streaming_chat_task(payload, request)
    task_manager = request.app.state.task_manager
    try:
        task_manager.submit(task)
    except TaskManagerClosedError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    return StreamingResponse(
        _sse_events(task),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache"},
    )


def _build_chat_task(
    payload: ChatPayload,
    request: Request,
) -> ChatTask:
    task_ref, inference_request = _build_chat_inference_request(payload)
    return ChatTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_streaming_chat_task(
    payload: ChatPayload,
    request: Request,
) -> StreamingChatTask:
    task_ref, inference_request = _build_chat_inference_request(payload)
    return StreamingChatTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_chat_inference_request(
    payload: ChatPayload,
) -> tuple[str, ChatInferenceRequest]:
    if not payload.messages:
        raise HTTPException(
            status_code=400,
            detail="chat requests must contain at least one message",
        )

    adapter = _adapter_record(payload.adapter)
    chat_request = ChatRequest(
        messages=tuple(
            ChatMessage(role=message.role, content=message.content)
            for message in payload.messages
        ),
        max_tokens=payload.max_tokens,
        temperature=payload.temperature,
        adapter_ref=adapter.adapter_ref if adapter is not None else None,
    )
    inference_request = ChatInferenceRequest(
        model_kind=payload.model_kind,
        model=_model_record(payload.model),
        chat=chat_request,
        adapter=adapter,
    )
    return payload.task_ref or uuid4().hex, inference_request


def _model_record(payload: ModelRecordPayload) -> ModelRecord:
    return ModelRecord(
        model_ref=payload.model_ref,
        source_path=Path(payload.source_path),
        primary_format=payload.primary_format,
        capabilities=frozenset(payload.capabilities),
        short_ref=payload.short_ref,
        source_repo=payload.source_repo,
        source_revision=payload.source_revision,
    )


def _adapter_record(payload: AdapterRecordPayload | None) -> AdapterRecord | None:
    if payload is None:
        return None
    return AdapterRecord(
        adapter_ref=payload.adapter_ref,
        source_path=Path(payload.source_path),
        adapter_format=payload.adapter_format,
        adapter_type=payload.adapter_type,
        short_ref=payload.short_ref,
        weight_file=payload.weight_file,
    )


def _sse_events(task: StreamingChatTask):
    try:
        for event in task.iter_events():
            yield _format_sse(event.event, event.data)
    finally:
        if not task.is_terminal:
            task.cancel()


def _format_sse(event: str, data: dict[str, object]) -> str:
    return f"event: {event}\ndata: {json.dumps(data, ensure_ascii=True)}\n\n"


def _http_exception(exc: BaseException) -> HTTPException:
    if isinstance(exc, FileNotFoundError):
        return HTTPException(status_code=404, detail=str(exc))
    if isinstance(exc, ValueError):
        return HTTPException(status_code=400, detail=str(exc))
    if isinstance(exc, RuntimeError) and "dependency" in str(exc).lower():
        return HTTPException(status_code=501, detail=str(exc))
    return HTTPException(status_code=500, detail=str(exc))
