from __future__ import annotations

import asyncio
from pathlib import Path
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field

from tentgent.runtime.backends.video_understanding import (
    UnsupportedMlxVideoModelError,
    VideoFocusRegion,
    VideoSamplingOptions,
    VideoUnderstandingContext,
    VideoUnderstandingModelKind,
    VideoUnderstandingOutputFormat,
    VideoUnderstandingRequest,
    normalize_video_understanding_request,
)
from tentgent.runtime.backends.records import ModelCapability
from tentgent.runtime.task.inference.video_understanding import (
    VideoUnderstandingInferenceRequest,
    VideoUnderstandingTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError

from .payloads import ModelRecordPayload
from ..managed_models import (
    infer_video_understanding_model_kind,
    resolve_request_model,
)


router = APIRouter(prefix="/v1/video")


class VideoSamplingPayload(BaseModel):
    sample_fps: float | None = None
    max_frames: int | None = None
    max_frame_edge: int | None = None
    clip_start_seconds: float | None = None
    clip_duration_seconds: float | None = None


class VideoFocusRegionPayload(BaseModel):
    x: float
    y: float
    width: float
    height: float
    label: str | None = None


class VideoUnderstandingContextPayload(BaseModel):
    transcript: str | None = None
    notes: list[str] = Field(default_factory=list)


class VideoUnderstandingPayload(BaseModel):
    task_ref: str | None = None
    model_kind: VideoUnderstandingModelKind | None = None
    model: ModelRecordPayload | None = None
    video_path: str
    prompt: str
    output_format: str | VideoUnderstandingOutputFormat = (
        VideoUnderstandingOutputFormat.TEXT
    )
    system_prompt: str | None = None
    max_tokens: int | None = None
    temperature: float | None = None
    sampling: VideoSamplingPayload | None = None
    focus_regions: list[VideoFocusRegionPayload] = Field(default_factory=list)
    context: VideoUnderstandingContextPayload | None = None


class VideoUnderstandingResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    output_format: VideoUnderstandingOutputFormat
    media_type: str
    text: str
    finish_reason: str
    sampled_frames: int | None = None


@router.post("/understanding")
async def video_understanding(
    payload: VideoUnderstandingPayload,
    request: Request,
) -> VideoUnderstandingResponsePayload:
    task = _build_video_understanding_task(payload, request)
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

    return VideoUnderstandingResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=task.request.model.model_ref,
        output_format=result.output_format,
        media_type=result.media_type,
        text=result.text,
        finish_reason=result.finish_reason,
        sampled_frames=result.sampled_frames,
    )


def _build_video_understanding_task(
    payload: VideoUnderstandingPayload,
    request: Request,
) -> VideoUnderstandingTask:
    task_ref, inference_request = _build_video_understanding_inference_request(
        payload,
        request,
    )
    return VideoUnderstandingTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_video_understanding_inference_request(
    payload: VideoUnderstandingPayload,
    request: Request,
) -> tuple[str, VideoUnderstandingInferenceRequest]:
    try:
        video_request = normalize_video_understanding_request(
            VideoUnderstandingRequest(
                video_path=Path(payload.video_path),
                prompt=payload.prompt,
                system_prompt=payload.system_prompt,
                output_format=payload.output_format,
                max_tokens=payload.max_tokens,
                temperature=payload.temperature,
                sampling=_sampling_options(payload.sampling),
                focus_regions=tuple(
                    VideoFocusRegion(
                        x=region.x,
                        y=region.y,
                        width=region.width,
                        height=region.height,
                        label=region.label,
                    )
                    for region in payload.focus_regions
                ),
                context=_context(payload.context),
            )
        )
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    model = resolve_request_model(
        payload.model,
        request,
        required_capability=ModelCapability.VIDEO_UNDERSTANDING,
    )
    model_kind = payload.model_kind or infer_video_understanding_model_kind(model)
    inference_request = VideoUnderstandingInferenceRequest(
        model_kind=model_kind,
        model=model,
        video=video_request,
    )
    return payload.task_ref or uuid4().hex, inference_request


def _sampling_options(payload: VideoSamplingPayload | None) -> VideoSamplingOptions:
    if payload is None:
        return VideoSamplingOptions()
    return VideoSamplingOptions(
        sample_fps=payload.sample_fps,
        max_frames=payload.max_frames,
        max_frame_edge=payload.max_frame_edge,
        clip_start_seconds=payload.clip_start_seconds,
        clip_duration_seconds=payload.clip_duration_seconds,
    )


def _context(
    payload: VideoUnderstandingContextPayload | None,
) -> VideoUnderstandingContext | None:
    if payload is None:
        return None
    return VideoUnderstandingContext(
        transcript=payload.transcript,
        notes=tuple(payload.notes),
    )


def _http_exception(exc: BaseException) -> HTTPException:
    if isinstance(exc, UnsupportedMlxVideoModelError):
        return HTTPException(status_code=501, detail=exc.to_http_detail())
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
            or "cannot apply a chat template" in message
        ):
            return HTTPException(status_code=501, detail=str(exc))
    return HTTPException(status_code=500, detail=str(exc))
