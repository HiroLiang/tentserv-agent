from __future__ import annotations

import asyncio
from pathlib import Path
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from tentgent.runtime.backends.image_generation import (
    DEFAULT_CONTROL_KIND,
    DEFAULT_GUIDANCE_SCALE,
    DEFAULT_HEIGHT,
    DEFAULT_STEPS,
    DEFAULT_WIDTH,
    ImageGenerationAdapterSelection,
    ImageGenerationControlSelection,
    ImageGenerationModelKind,
    ImageGenerationOutputFormat,
    ImageGenerationRequest,
    ImageGenerationWorkflowKind,
    normalize_image_generation_request,
)
from tentgent.runtime.backends.records import ModelCapability
from tentgent.runtime.task.inference.image_generation import (
    ImageGenerationInferenceRequest,
    ImageGenerationTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError

from .payloads import ModelRecordPayload
from ..managed_models import infer_image_generation_model_kind, resolve_request_model


router = APIRouter(prefix="/v1/images")


class ImageAdapterPayload(BaseModel):
    adapter_ref: str
    source_path: str
    lora_scale: float = 1.0
    weight_file: str | None = None


class ImageControlAdapterPayload(BaseModel):
    control_ref: str
    source_path: str
    control_kind: str = DEFAULT_CONTROL_KIND


class ImageBasePayload(BaseModel):
    task_ref: str | None = None
    model_kind: ImageGenerationModelKind | None = None
    model: ModelRecordPayload | None = None
    prompt: str
    output_path: str
    output_format: str | ImageGenerationOutputFormat = ImageGenerationOutputFormat.PNG
    negative_prompt: str | None = None
    width: int = DEFAULT_WIDTH
    height: int = DEFAULT_HEIGHT
    steps: int = DEFAULT_STEPS
    guidance_scale: float = DEFAULT_GUIDANCE_SCALE
    seed: int | None = None
    adapter: ImageAdapterPayload | None = None


class ImageGenerationPayload(ImageBasePayload):
    pass


class ImageTransformPayload(ImageBasePayload):
    input_image_path: str
    input_image_media_type: str | None = None
    strength: float | None = None


class ImageInpaintPayload(ImageBasePayload):
    input_image_path: str
    input_image_media_type: str | None = None
    mask_image_path: str
    mask_image_media_type: str | None = None
    strength: float | None = None


class ImageControlPayload(ImageBasePayload):
    control_image_path: str
    control_image_media_type: str | None = None
    control_kind: str | None = None
    control_strength: float | None = None
    control: ImageControlAdapterPayload


class ImageGenerationResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    output_format: ImageGenerationOutputFormat
    media_type: str
    output_path: str
    total_bytes: int
    width: int
    height: int
    seed: int | None


@router.post("/generations")
async def image_generations(
    payload: ImageGenerationPayload,
    request: Request,
) -> ImageGenerationResponsePayload:
    return await _run_image_task(
        payload,
        request,
        workflow_kind=ImageGenerationWorkflowKind.TEXT_TO_IMAGE,
    )


@router.post("/transforms")
async def image_transforms(
    payload: ImageTransformPayload,
    request: Request,
) -> ImageGenerationResponsePayload:
    return await _run_image_task(
        payload,
        request,
        workflow_kind=ImageGenerationWorkflowKind.IMAGE_TO_IMAGE,
    )


@router.post("/inpaint")
async def image_inpaint(
    payload: ImageInpaintPayload,
    request: Request,
) -> ImageGenerationResponsePayload:
    return await _run_image_task(
        payload,
        request,
        workflow_kind=ImageGenerationWorkflowKind.INPAINT,
    )


@router.post("/control")
async def image_control(
    payload: ImageControlPayload,
    request: Request,
) -> ImageGenerationResponsePayload:
    return await _run_image_task(
        payload,
        request,
        workflow_kind=ImageGenerationWorkflowKind.CONTROL,
    )


async def _run_image_task(
    payload: ImageBasePayload,
    request: Request,
    *,
    workflow_kind: ImageGenerationWorkflowKind,
) -> ImageGenerationResponsePayload:
    task = _build_image_task(payload, request, workflow_kind=workflow_kind)
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

    return ImageGenerationResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=task.request.model.model_ref,
        output_format=result.output_format,
        media_type=result.media_type,
        output_path=str(result.output_path),
        total_bytes=result.total_bytes,
        width=result.width,
        height=result.height,
        seed=result.seed,
    )


def _build_image_task(
    payload: ImageBasePayload,
    request: Request,
    *,
    workflow_kind: ImageGenerationWorkflowKind,
) -> ImageGenerationTask:
    task_ref, inference_request = _build_image_inference_request(
        payload,
        request,
        workflow_kind=workflow_kind,
    )
    return ImageGenerationTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_image_inference_request(
    payload: ImageBasePayload,
    request: Request,
    *,
    workflow_kind: ImageGenerationWorkflowKind,
) -> tuple[str, ImageGenerationInferenceRequest]:
    model = resolve_request_model(
        payload.model,
        request,
        required_capability=ModelCapability.IMAGE_GENERATION,
    )
    model_kind = payload.model_kind or infer_image_generation_model_kind(
        model,
        workflow_kind,
    )
    _validate_model_kind_workflow(model_kind, workflow_kind)
    try:
        image_request = normalize_image_generation_request(
            _image_request(payload, workflow_kind=workflow_kind)
        )
    except FileNotFoundError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    except (FileExistsError, ValueError) as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except RuntimeError as exc:
        raise _http_exception(exc) from exc

    inference_request = ImageGenerationInferenceRequest(
        model_kind=model_kind,
        model=model,
        image=image_request,
    )
    return payload.task_ref or uuid4().hex, inference_request


def _image_request(
    payload: ImageBasePayload,
    *,
    workflow_kind: ImageGenerationWorkflowKind,
) -> ImageGenerationRequest:
    return ImageGenerationRequest(
        workflow_kind=workflow_kind,
        prompt=payload.prompt,
        negative_prompt=payload.negative_prompt,
        output_path=Path(payload.output_path),
        output_format=payload.output_format,
        input_image_path=_maybe_path(getattr(payload, "input_image_path", None)),
        input_image_media_type=getattr(payload, "input_image_media_type", None),
        mask_image_path=_maybe_path(getattr(payload, "mask_image_path", None)),
        mask_image_media_type=getattr(payload, "mask_image_media_type", None),
        strength=getattr(payload, "strength", None),
        control_image_path=_maybe_path(getattr(payload, "control_image_path", None)),
        control_image_media_type=getattr(payload, "control_image_media_type", None),
        control_kind=getattr(payload, "control_kind", None),
        control_strength=getattr(payload, "control_strength", None),
        control=_control_selection(getattr(payload, "control", None)),
        adapter=_adapter_selection(payload.adapter),
        width=payload.width,
        height=payload.height,
        steps=payload.steps,
        guidance_scale=payload.guidance_scale,
        seed=payload.seed,
    )


def _adapter_selection(
    payload: ImageAdapterPayload | None,
) -> ImageGenerationAdapterSelection | None:
    if payload is None:
        return None
    return ImageGenerationAdapterSelection(
        adapter_ref=payload.adapter_ref,
        source_path=Path(payload.source_path),
        lora_scale=payload.lora_scale,
        weight_file=payload.weight_file,
    )


def _control_selection(
    payload: ImageControlAdapterPayload | None,
) -> ImageGenerationControlSelection | None:
    if payload is None:
        return None
    return ImageGenerationControlSelection(
        control_ref=payload.control_ref,
        source_path=Path(payload.source_path),
        control_kind=payload.control_kind.strip().lower(),
    )


def _maybe_path(value: str | None) -> Path | None:
    if value is None:
        return None
    return Path(value)


def _validate_model_kind_workflow(
    model_kind: ImageGenerationModelKind,
    workflow_kind: ImageGenerationWorkflowKind,
) -> None:
    expected = _model_kinds_for_workflow(workflow_kind)
    if model_kind in expected:
        return
    expected_label = ", ".join(kind.value for kind in expected)
    raise HTTPException(
        status_code=400,
        detail=(
            f"image model kind `{model_kind.value}` cannot serve workflow "
            f"`{workflow_kind.value}`; expected one of: {expected_label}"
        ),
    )


def _model_kinds_for_workflow(
    workflow_kind: ImageGenerationWorkflowKind,
) -> tuple[ImageGenerationModelKind, ...]:
    if workflow_kind == ImageGenerationWorkflowKind.TEXT_TO_IMAGE:
        return (
            ImageGenerationModelKind.DIFFUSERS_TEXT_TO_IMAGE,
            ImageGenerationModelKind.MLX_DIFFUSION_TEXT_TO_IMAGE,
        )
    if workflow_kind == ImageGenerationWorkflowKind.IMAGE_TO_IMAGE:
        return (
            ImageGenerationModelKind.DIFFUSERS_IMAGE_TO_IMAGE,
            ImageGenerationModelKind.MLX_DIFFUSION_IMAGE_TO_IMAGE,
        )
    if workflow_kind == ImageGenerationWorkflowKind.INPAINT:
        return (
            ImageGenerationModelKind.DIFFUSERS_INPAINT,
            ImageGenerationModelKind.MLX_DIFFUSION_INPAINT,
        )
    if workflow_kind == ImageGenerationWorkflowKind.CONTROL:
        return (ImageGenerationModelKind.DIFFUSERS_CONTROL,)
    raise AssertionError(f"unhandled image workflow kind: {workflow_kind}")


def _http_exception(exc: BaseException) -> HTTPException:
    if isinstance(exc, FileNotFoundError):
        return HTTPException(status_code=404, detail=str(exc))
    if isinstance(exc, (FileExistsError, ValueError)):
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
            or "requires a recent runtime" in message
        ):
            return HTTPException(status_code=501, detail=str(exc))
    return HTTPException(status_code=500, detail=str(exc))
