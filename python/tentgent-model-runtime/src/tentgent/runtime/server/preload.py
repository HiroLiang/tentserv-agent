from __future__ import annotations

import asyncio
import json
from typing import Any

from fastapi import HTTPException, Request
from pydantic import BaseModel, ConfigDict, StrictStr, ValidationError, field_validator
from tentgent.runtime.backends.records import ModelCapability
from tentgent.runtime.backends.resource_manager import ModelResourceCleanupError
from tentgent.runtime.task.manager import TaskManagerClosedError
from tentgent.runtime.task.preload import PreloadTask

from .managed_models import infer_preload_model_kind_for_capability

PRELOAD_WAIT_SECONDS = 300.0
_METADATA_ONLY_KINDS = frozenset({"mlx-embedding", "mlx-rerank"})


class PreloadRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    task_ref: StrictStr
    process_token: StrictStr

    @field_validator("task_ref", "process_token")
    @classmethod
    def non_empty(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("must be a non-empty string")
        return value


async def preload_bound_model(request: Request) -> dict[str, Any]:
    try:
        payload = PreloadRequest.model_validate(await request.json())
    except (ValidationError, json.JSONDecodeError, UnicodeDecodeError):
        raise _error(
            422,
            "invalid_preload_request",
            "preload requires only non-empty string task_ref and process_token fields",
        ) from None

    state = request.app.state
    process_token = state.lifecycle.process_token
    if not process_token or payload.process_token != process_token:
        raise _error(
            409, "runtime_generation_mismatch", "runtime generation does not match"
        )

    bound_model = state.bound_model
    if bound_model is None:
        raise _error(
            400, "preload_unbound_runtime", "preload requires a model-bound runtime"
        )
    try:
        model_kind = infer_preload_model_kind_for_capability(
            bound_model.capability,
            bound_model.model,
        )
    except HTTPException as error:
        raise _error(501, "preload_unsupported", str(error.detail)) from error
    if model_kind is None or model_kind in _METADATA_ONLY_KINDS:
        raise _error(
            501,
            "preload_unsupported",
            f"preload is not supported for {bound_model.capability.value} with this backend; "
            "use lazy loading",
        )
    if (
        bound_model.model.capabilities
        and ModelCapability(bound_model.capability.value)
        not in bound_model.model.capabilities
    ):
        raise _error(
            501, "preload_unsupported", "bound model does not advertise this capability"
        )

    task = PreloadTask(
        task_ref=payload.task_ref,
        model_kind=model_kind,
        model=bound_model.model,
        resource_manager=state.resource_manager,
    )
    try:
        handle = state.task_manager.submit(task)
    except TaskManagerClosedError:
        raise _error(
            409, "runtime_closing", "runtime is closing or shut down"
        ) from None
    except ValueError:
        raise _error(
            409, "preload_task_exists", "task_ref is already tracked"
        ) from None

    future = asyncio.wrap_future(handle.future)
    # Shield queued as well as running work. Always retrieve late failures so a
    # timed-out/canceled HTTP waiter does not cause unobserved-future warnings.
    future.add_done_callback(_consume_completion)
    budget = asyncio.timeout(PRELOAD_WAIT_SECONDS)
    try:
        async with budget:
            await asyncio.shield(future)
    except TimeoutError as error:
        if not budget.expired():
            raise _error(
                500, "preload_failed", str(error), task_ref=payload.task_ref
            ) from error
        raise _error(
            504,
            "preload_wait_timeout",
            "preload wait expired; accepted loading continues until completion",
            task_ref=payload.task_ref,
        ) from None
    except Exception as error:
        status = 501 if _unsupported_error(error) else 500
        raise _error(
            status,
            "preload_unsupported" if status == 501 else "preload_failed",
            str(error),
            task_ref=payload.task_ref,
        ) from error

    return {
        "status": "done",
        "task_ref": payload.task_ref,
        "model_ref": bound_model.model.model_ref,
        "capability": bound_model.capability.value,
        "process_token": process_token,
    }


def _consume_completion(future: asyncio.Future[None]) -> None:
    if not future.cancelled():
        future.exception()


def _unsupported_error(error: Exception) -> bool:
    if isinstance(error, ModelResourceCleanupError):
        return False
    return isinstance(error, (ImportError, NotImplementedError)) or (
        isinstance(error, RuntimeError)
        and str(error).startswith("Python model runtime dependency is not installed.")
    )


def _error(
    status: int,
    code: str,
    message: str,
    *,
    task_ref: str | None = None,
) -> HTTPException:
    detail = {"code": code, "message": message}
    if task_ref is not None:
        detail["task_ref"] = task_ref
    return HTTPException(status_code=status, detail=detail)
