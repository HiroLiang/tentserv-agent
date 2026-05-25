from __future__ import annotations

import asyncio
from pathlib import Path
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from tentgent.runtime.backends.vision_chat import (
    VisionChatModelKind,
    VisionChatOutputFormat,
    VisionChatRequest,
    normalize_vision_chat_request,
)
from tentgent.runtime.task.inference.vision_chat import (
    VisionChatInferenceRequest,
    VisionChatTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError

from .payloads import ModelRecordPayload, model_record


router = APIRouter(prefix="/v1/vision")


class VisionChatPayload(BaseModel):
    task_ref: str | None = None
    model_kind: VisionChatModelKind
    model: ModelRecordPayload
    image_path: str
    prompt: str
    output_format: str | VisionChatOutputFormat = VisionChatOutputFormat.TEXT
    image_media_type: str | None = None
    system_prompt: str | None = None
    max_tokens: int | None = None
    temperature: float | None = None


class VisionChatResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    output_format: VisionChatOutputFormat
    media_type: str
    text: str
    finish_reason: str


@router.post("/chat")
async def vision_chat(
    payload: VisionChatPayload,
    request: Request,
) -> VisionChatResponsePayload:
    task = _build_vision_chat_task(payload, request)
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

    return VisionChatResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=payload.model.model_ref,
        output_format=result.output_format,
        media_type=result.media_type,
        text=result.text,
        finish_reason=result.finish_reason,
    )


def _build_vision_chat_task(
    payload: VisionChatPayload,
    request: Request,
) -> VisionChatTask:
    task_ref, inference_request = _build_vision_chat_inference_request(payload)
    return VisionChatTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_vision_chat_inference_request(
    payload: VisionChatPayload,
) -> tuple[str, VisionChatInferenceRequest]:
    try:
        vision_request = normalize_vision_chat_request(
            VisionChatRequest(
                image_path=Path(payload.image_path),
                image_media_type=payload.image_media_type,
                prompt=payload.prompt,
                system_prompt=payload.system_prompt,
                output_format=payload.output_format,
                max_tokens=payload.max_tokens,
                temperature=payload.temperature,
            )
        )
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    inference_request = VisionChatInferenceRequest(
        model_kind=payload.model_kind,
        model=model_record(payload.model),
        vision=vision_request,
    )
    return payload.task_ref or uuid4().hex, inference_request


def _http_exception(exc: BaseException) -> HTTPException:
    if isinstance(exc, FileNotFoundError):
        return HTTPException(status_code=404, detail=str(exc))
    if isinstance(exc, ValueError):
        return HTTPException(status_code=400, detail=str(exc))
    if isinstance(exc, NotImplementedError):
        return HTTPException(status_code=501, detail=str(exc))
    if isinstance(exc, RuntimeError):
        message = str(exc).lower()
        if (
            "dependency" in message
            or "optional" in message
            or "not installed" in message
            or "install it" in message
            or "does not provide" in message
        ):
            return HTTPException(status_code=501, detail=str(exc))
    return HTTPException(status_code=500, detail=str(exc))
