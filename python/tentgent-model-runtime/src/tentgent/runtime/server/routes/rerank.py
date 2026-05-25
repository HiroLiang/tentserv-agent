from __future__ import annotations

import asyncio
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from tentgent.runtime.backends.rerank import (
    RerankModelKind,
    RerankRequest,
)
from tentgent.runtime.task.inference.rerank import (
    RerankInferenceRequest,
    RerankTask,
)
from tentgent.runtime.task.manager import TaskManagerClosedError

from .payloads import ModelRecordPayload, model_record


router = APIRouter(prefix="/v1")


class RerankPayload(BaseModel):
    task_ref: str | None = None
    model_kind: RerankModelKind
    model: ModelRecordPayload
    query: str
    documents: list[str]
    top_n: int | None = None


class RerankScorePayload(BaseModel):
    index: int
    score: float


class RerankResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    data: list[RerankScorePayload]


@router.post("/rerank")
async def rerank(payload: RerankPayload, request: Request) -> RerankResponsePayload:
    task = _build_rerank_task(payload, request)
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

    return RerankResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=payload.model.model_ref,
        data=[
            RerankScorePayload(index=item.index, score=item.score)
            for item in result.data
        ],
    )


def _build_rerank_task(payload: RerankPayload, request: Request) -> RerankTask:
    task_ref, inference_request = _build_rerank_inference_request(payload)
    return RerankTask(
        task_ref=task_ref,
        request=inference_request,
        resources=request.app.state.resource_manager,
    )


def _build_rerank_inference_request(
    payload: RerankPayload,
) -> tuple[str, RerankInferenceRequest]:
    query = _rerank_query(payload.query)
    documents = _rerank_documents(payload.documents)
    top_n = _rerank_top_n(payload.top_n, document_count=len(documents))
    inference_request = RerankInferenceRequest(
        model_kind=payload.model_kind,
        model=model_record(payload.model),
        rerank=RerankRequest(query=query, documents=documents, top_n=top_n),
    )
    return payload.task_ref or uuid4().hex, inference_request


def _rerank_query(query: str) -> str:
    if not isinstance(query, str) or not query.strip():
        raise HTTPException(
            status_code=400,
            detail="`query` must be a non-empty string",
        )
    return query


def _rerank_documents(documents: list[str]) -> tuple[str, ...]:
    if not documents:
        raise HTTPException(status_code=400, detail="`documents` must not be empty")
    if any(not isinstance(document, str) or not document.strip() for document in documents):
        raise HTTPException(
            status_code=400,
            detail="`documents` strings must not be empty",
        )
    return tuple(documents)


def _rerank_top_n(top_n: int | None, *, document_count: int) -> int | None:
    if top_n is not None and (top_n < 1 or top_n > document_count):
        raise HTTPException(
            status_code=400,
            detail="`top_n` must be between 1 and document count",
        )
    return top_n


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
