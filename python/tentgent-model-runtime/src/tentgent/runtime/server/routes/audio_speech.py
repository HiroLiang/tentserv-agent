from __future__ import annotations

import asyncio
from pathlib import Path
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from tentgent.runtime.backends.audio_speech import (
    AudioSpeechModelKind,
    AudioSpeechOutputFormat,
    AudioSpeechRequest,
    validate_audio_speech_text,
)
from tentgent.runtime.backends.records import ModelCapability
from tentgent.runtime.task.inference.audio_speech import (
    AudioSpeechInferenceRequest,
    AudioSpeechTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError

from .payloads import ModelRecordPayload
from ..managed_models import infer_audio_speech_model_kind, resolve_request_model


router = APIRouter(prefix="/v1/audio")


class AudioSpeechPayload(BaseModel):
    task_ref: str | None = None
    model_kind: AudioSpeechModelKind | None = None
    model: ModelRecordPayload | None = None
    text: str
    output_path: str
    output_format: AudioSpeechOutputFormat = AudioSpeechOutputFormat.WAV
    language: str | None = None
    voice: str | None = None


class AudioSpeechResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    output_format: AudioSpeechOutputFormat
    media_type: str
    output_path: str
    total_bytes: int
    sample_rate: int | None


@router.post("/speech")
async def audio_speech(
    payload: AudioSpeechPayload,
    request: Request,
) -> AudioSpeechResponsePayload:
    task = _build_audio_speech_task(payload, request)
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

    return AudioSpeechResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=task.request.model.model_ref,
        output_format=result.output_format,
        media_type=result.media_type,
        output_path=str(result.output_path),
        total_bytes=result.total_bytes,
        sample_rate=result.sample_rate,
    )


def _build_audio_speech_task(
    payload: AudioSpeechPayload,
    request: Request,
) -> AudioSpeechTask:
    task_ref, inference_request = _build_audio_speech_inference_request(
        payload,
        request,
    )
    return AudioSpeechTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_audio_speech_inference_request(
    payload: AudioSpeechPayload,
    request: Request,
) -> tuple[str, AudioSpeechInferenceRequest]:
    try:
        text = validate_audio_speech_text(payload.text)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    model = resolve_request_model(
        payload.model,
        request,
        required_capability=ModelCapability.AUDIO_SPEECH,
    )
    model_kind = payload.model_kind or infer_audio_speech_model_kind(model)
    speech_request = AudioSpeechRequest(
        text=text,
        output_path=Path(payload.output_path).expanduser().resolve(),
        output_format=payload.output_format,
        language=_optional_text(payload.language),
        voice=_optional_text(payload.voice),
    )
    inference_request = AudioSpeechInferenceRequest(
        model_kind=model_kind,
        model=model,
        speech=speech_request,
    )
    return payload.task_ref or uuid4().hex, inference_request


def _optional_text(value: str | None) -> str | None:
    if value is None:
        return None
    stripped = value.strip()
    return stripped or None


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
        ):
            return HTTPException(status_code=501, detail=str(exc))
    return HTTPException(status_code=500, detail=str(exc))
