from __future__ import annotations

import asyncio
from pathlib import Path
from uuid import uuid4

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field

from tentgent.runtime.backends.lora_tuning import (
    LoraBackendConfig,
    LoraCheckpointConfig,
    LoraConfig,
    LoraDatasetConfig,
    LoraOptimizationConfig,
    LoraTuningBackendKind,
    LoraTuningRequest,
    normalize_lora_tuning_request,
)
from tentgent.runtime.task.manager import TaskManagerClosedError
from tentgent.runtime.task.training.lora_tuning import (
    LoraTuningTask,
    LoraTuningTaskRequest,
)

from .payloads import ModelRecordPayload, model_record


router = APIRouter(prefix="/v1/tuning/lora")


class LoraDatasetPayload(BaseModel):
    source_path: str
    max_seq_length: int = 2048
    mask_prompt: bool = True


class LoraPayload(BaseModel):
    rank: int = 8
    alpha: int | None = None
    dropout: float = 0.0
    scale: float = 20.0
    target_modules: list[str] = Field(default_factory=list)


class LoraOptimizationPayload(BaseModel):
    max_steps: int = 100
    batch_size: int = 1
    learning_rate: float = 2e-4
    weight_decay: float = 0.0
    gradient_accumulation_steps: int = 1
    optimizer: str = "adamw"
    seed: int = 0


class LoraCheckpointPayload(BaseModel):
    log_every_steps: int = 10
    eval_every_steps: int = 200
    save_every_steps: int = 100


class LoraBackendConfigPayload(BaseModel):
    peft: dict[str, object] = Field(default_factory=dict)
    mlx: dict[str, object] = Field(default_factory=dict)


class LoraTuningPayload(BaseModel):
    task_ref: str | None = None
    backend: LoraTuningBackendKind
    model: ModelRecordPayload
    dataset: LoraDatasetPayload
    output_dir: str
    lora: LoraPayload = Field(default_factory=LoraPayload)
    optimization: LoraOptimizationPayload = Field(default_factory=LoraOptimizationPayload)
    checkpoint: LoraCheckpointPayload = Field(default_factory=LoraCheckpointPayload)
    backend_config: LoraBackendConfigPayload = Field(
        default_factory=LoraBackendConfigPayload
    )
    plan_ref: str | None = None
    run_ref: str | None = None


class LoraTuningResponsePayload(BaseModel):
    task_ref: str
    status: str
    model_ref: str
    backend: LoraTuningBackendKind
    output_dir: str
    adapter_path: str
    adapter_file: str | None
    finish_reason: str
    events: list[dict[str, object]]


@router.post("/runs")
async def run_lora_tuning(
    payload: LoraTuningPayload,
    request: Request,
) -> LoraTuningResponsePayload:
    try:
        task = _build_lora_tuning_task(payload, request)
    except BaseException as exc:
        raise _http_exception(exc) from exc

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

    return LoraTuningResponsePayload(
        task_ref=handle.task_ref,
        status=task.status.value,
        model_ref=result.model_ref,
        backend=result.backend,
        output_dir=str(result.output_dir),
        adapter_path=str(result.adapter_path),
        adapter_file=str(result.adapter_file) if result.adapter_file else None,
        finish_reason=result.finish_reason,
        events=list(result.events),
    )


def _build_lora_tuning_task(
    payload: LoraTuningPayload,
    request: Request,
) -> LoraTuningTask:
    task_ref, tuning_request = _build_lora_tuning_request(payload)
    return LoraTuningTask(
        task_ref=task_ref,
        request=LoraTuningTaskRequest(
            backend=tuning_request.backend,
            model=tuning_request.model,
            tuning=tuning_request,
        ),
        resources=request.app.state.resource_manager,
    )


def _build_lora_tuning_request(
    payload: LoraTuningPayload,
) -> tuple[str, LoraTuningRequest]:
    model = model_record(payload.model)
    tuning_request = normalize_lora_tuning_request(
        LoraTuningRequest(
            backend=payload.backend,
            model=model,
            dataset=LoraDatasetConfig(
                source_path=Path(payload.dataset.source_path),
                max_seq_length=payload.dataset.max_seq_length,
                mask_prompt=payload.dataset.mask_prompt,
            ),
            output_dir=Path(payload.output_dir),
            lora=LoraConfig(
                rank=payload.lora.rank,
                alpha=payload.lora.alpha,
                dropout=payload.lora.dropout,
                scale=payload.lora.scale,
                target_modules=tuple(payload.lora.target_modules),
            ),
            optimization=LoraOptimizationConfig(
                max_steps=payload.optimization.max_steps,
                batch_size=payload.optimization.batch_size,
                learning_rate=payload.optimization.learning_rate,
                weight_decay=payload.optimization.weight_decay,
                gradient_accumulation_steps=(
                    payload.optimization.gradient_accumulation_steps
                ),
                optimizer=payload.optimization.optimizer,
                seed=payload.optimization.seed,
            ),
            checkpoint=LoraCheckpointConfig(
                log_every_steps=payload.checkpoint.log_every_steps,
                eval_every_steps=payload.checkpoint.eval_every_steps,
                save_every_steps=payload.checkpoint.save_every_steps,
            ),
            backend_config=LoraBackendConfig(
                peft=dict(payload.backend_config.peft),
                mlx=dict(payload.backend_config.mlx),
            ),
            plan_ref=payload.plan_ref,
            run_ref=payload.run_ref,
        )
    )
    return payload.task_ref or uuid4().hex, tuning_request


def _http_exception(exc: BaseException) -> HTTPException:
    if isinstance(exc, FileNotFoundError):
        return HTTPException(status_code=404, detail=str(exc))
    if isinstance(exc, ValueError):
        return HTTPException(status_code=400, detail=str(exc))
    if isinstance(exc, RuntimeError) and "dependency" in str(exc).lower():
        return HTTPException(status_code=501, detail=str(exc))
    if isinstance(exc, NotImplementedError):
        return HTTPException(status_code=501, detail=str(exc))
    return HTTPException(status_code=500, detail=str(exc))
