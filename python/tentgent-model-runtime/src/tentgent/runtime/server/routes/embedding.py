from __future__ import annotations

import asyncio
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from tentgent.runtime.backends.embedding import (
    EmbeddingModelKind,
    EmbeddingRequest,
)
from tentgent.runtime.backends.records import ModelCapability
from tentgent.runtime.task.inference.embedding import (
    EmbeddingInferenceRequest,
    EmbeddingTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError

from .payloads import ModelRecordPayload
from ..managed_models import infer_embedding_model_kind, resolve_request_model


router = APIRouter(prefix="/v1")


class EmbeddingPayload(BaseModel):
    task_ref: str | None = None
    model_kind: EmbeddingModelKind | None = None
    model: ModelRecordPayload | None = None
    input: str | list[str]


class EmbeddingVectorPayload(BaseModel):
    index: int
    embedding: list[float]


class EmbeddingResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    data: list[EmbeddingVectorPayload]


@router.post("/embeddings")
async def embeddings(
    payload: EmbeddingPayload,
    request: Request,
) -> EmbeddingResponsePayload:
    task = _build_embedding_task(payload, request)
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

    return EmbeddingResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=task.request.model.model_ref,
        data=[
            EmbeddingVectorPayload(index=item.index, embedding=item.embedding)
            for item in result.data
        ],
    )


def _build_embedding_task(
    payload: EmbeddingPayload,
    request: Request,
) -> EmbeddingTask:
    task_ref, inference_request = _build_embedding_inference_request(payload, request)
    return EmbeddingTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_embedding_inference_request(
    payload: EmbeddingPayload,
    request: Request,
) -> tuple[str, EmbeddingInferenceRequest]:
    inputs = _embedding_inputs(payload.input)
    model = resolve_request_model(
        payload.model,
        request,
        required_capability=ModelCapability.EMBEDDING,
    )
    model_kind = payload.model_kind or infer_embedding_model_kind(model)
    inference_request = EmbeddingInferenceRequest(
        model_kind=model_kind,
        model=model,
        embedding=EmbeddingRequest(inputs=inputs),
    )
    return payload.task_ref or uuid4().hex, inference_request


def _embedding_inputs(raw_input: str | list[str]) -> tuple[str, ...]:
    if isinstance(raw_input, str):
        inputs = (raw_input,)
    else:
        inputs = tuple(raw_input)

    if not inputs:
        raise HTTPException(status_code=400, detail="`input` must not be empty")
    if any(not isinstance(item, str) or not item.strip() for item in inputs):
        raise HTTPException(
            status_code=400,
            detail="`input` strings must not be empty",
        )
    return inputs


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
