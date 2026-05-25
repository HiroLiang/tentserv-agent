from __future__ import annotations

import asyncio
from pathlib import Path
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from tentgent.runtime.backends.audio_transcription import (
    AudioTranscriptionModelKind,
    AudioTranscriptionOutputFormat,
    AudioTranscriptionRequest,
)
from tentgent.runtime.task.inference.audio_transcription import (
    AudioTranscriptionInferenceRequest,
    AudioTranscriptionTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError

from .payloads import ModelRecordPayload, model_record


router = APIRouter(prefix="/v1/audio")


class AudioTranscriptionPayload(BaseModel):
    task_ref: str | None = None
    model_kind: AudioTranscriptionModelKind
    model: ModelRecordPayload
    input_path: str
    output_path: str
    output_format: AudioTranscriptionOutputFormat = AudioTranscriptionOutputFormat.TEXT
    language: str | None = None
    timestamps: bool = False


class AudioTranscriptionResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    output_format: AudioTranscriptionOutputFormat
    media_type: str
    output_path: str
    total_bytes: int
    text: str | None


@router.post("/transcriptions")
async def audio_transcriptions(
    payload: AudioTranscriptionPayload,
    request: Request,
) -> AudioTranscriptionResponsePayload:
    task = _build_audio_transcription_task(payload, request)
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

    return AudioTranscriptionResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=payload.model.model_ref,
        output_format=result.output_format,
        media_type=result.media_type,
        output_path=str(result.output_path),
        total_bytes=result.total_bytes,
        text=result.text,
    )


def _build_audio_transcription_task(
    payload: AudioTranscriptionPayload,
    request: Request,
) -> AudioTranscriptionTask:
    task_ref, inference_request = _build_audio_transcription_inference_request(payload)
    return AudioTranscriptionTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_audio_transcription_inference_request(
    payload: AudioTranscriptionPayload,
) -> tuple[str, AudioTranscriptionInferenceRequest]:
    input_path = Path(payload.input_path).expanduser().resolve()
    if not input_path.is_file():
        raise HTTPException(
            status_code=404,
            detail=f"audio input path `{input_path}` was not found",
        )

    transcription_request = AudioTranscriptionRequest(
        input_path=input_path,
        output_path=Path(payload.output_path).expanduser().resolve(),
        output_format=payload.output_format,
        language=_optional_text(payload.language),
        timestamps=payload.timestamps,
    )
    inference_request = AudioTranscriptionInferenceRequest(
        model_kind=payload.model_kind,
        model=model_record(payload.model),
        transcription=transcription_request,
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
    if isinstance(exc, RuntimeError) and "dependency" in str(exc).lower():
        return HTTPException(status_code=501, detail=str(exc))
    return HTTPException(status_code=500, detail=str(exc))
