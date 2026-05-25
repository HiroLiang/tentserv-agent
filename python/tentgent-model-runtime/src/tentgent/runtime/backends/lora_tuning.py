from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass, field, replace
from enum import StrEnum
from pathlib import Path
from typing import Any

from .base import BackendModel
from .records import ModelCapability, ModelFormat, ModelRecord


LoraTuningEvent = dict[str, Any]
LoraTuningEventSink = Callable[[LoraTuningEvent], None]


class LoraTuningBackendKind(StrEnum):
    PEFT = "peft"
    MLX = "mlx"


@dataclass(frozen=True, slots=True)
class LoraDatasetConfig:
    source_path: Path
    max_seq_length: int = 2048
    mask_prompt: bool = True


@dataclass(frozen=True, slots=True)
class LoraConfig:
    rank: int = 8
    alpha: int | None = None
    dropout: float = 0.0
    scale: float = 20.0
    target_modules: tuple[str, ...] = ()


@dataclass(frozen=True, slots=True)
class LoraOptimizationConfig:
    max_steps: int = 100
    batch_size: int = 1
    learning_rate: float = 2e-4
    weight_decay: float = 0.0
    gradient_accumulation_steps: int = 1
    optimizer: str = "adamw"
    seed: int = 0


@dataclass(frozen=True, slots=True)
class LoraCheckpointConfig:
    log_every_steps: int = 10
    eval_every_steps: int = 200
    save_every_steps: int = 100


@dataclass(frozen=True, slots=True)
class LoraBackendConfig:
    peft: dict[str, Any] = field(default_factory=dict)
    mlx: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True, slots=True)
class LoraTuningRequest:
    backend: LoraTuningBackendKind
    model: ModelRecord
    dataset: LoraDatasetConfig
    output_dir: Path
    lora: LoraConfig = field(default_factory=LoraConfig)
    optimization: LoraOptimizationConfig = field(default_factory=LoraOptimizationConfig)
    checkpoint: LoraCheckpointConfig = field(default_factory=LoraCheckpointConfig)
    backend_config: LoraBackendConfig = field(default_factory=LoraBackendConfig)
    plan_ref: str | None = None
    run_ref: str | None = None


@dataclass(frozen=True, slots=True)
class LoraTuningResult:
    backend: LoraTuningBackendKind
    model_ref: str
    output_dir: Path
    adapter_path: Path
    adapter_file: Path | None
    events: tuple[LoraTuningEvent, ...] = ()
    finish_reason: str = "stop"


class LoraTuningBackendModel(BackendModel, ABC):
    @abstractmethod
    def run_lora_tuning(
        self,
        request: LoraTuningRequest,
        *,
        emit: LoraTuningEventSink,
    ) -> LoraTuningResult:
        raise NotImplementedError


def build_lora_tuning_model(kind: Any) -> LoraTuningBackendModel:
    try:
        tuning_kind = kind if isinstance(kind, LoraTuningBackendKind) else (
            LoraTuningBackendKind(kind)
        )
    except ValueError as exc:
        raise ValueError(f"unsupported LoRA tuning backend `{kind}`") from exc

    if tuning_kind == LoraTuningBackendKind.PEFT:
        from .transformers import TransformersPeftLoraTuningModel

        return TransformersPeftLoraTuningModel()
    if tuning_kind == LoraTuningBackendKind.MLX:
        from .mlx import MlxLoraTuningModel

        return MlxLoraTuningModel()

    raise ValueError(f"unsupported LoRA tuning backend `{kind}`")


def normalize_lora_tuning_request(request: LoraTuningRequest) -> LoraTuningRequest:
    ensure_lora_trainable_model(request.model, request.backend)
    model = normalize_lora_model_record(request.model)
    dataset = normalize_lora_dataset_config(request.dataset)
    output_dir = request.output_dir.expanduser().resolve()
    if output_dir.exists() and not output_dir.is_dir():
        raise ValueError(f"LoRA output path `{output_dir}` is not a directory")

    return replace(
        request,
        model=model,
        dataset=dataset,
        output_dir=output_dir,
        lora=normalize_lora_config(request.lora),
        optimization=normalize_lora_optimization_config(request.optimization),
        checkpoint=normalize_lora_checkpoint_config(request.checkpoint),
    )


def ensure_lora_trainable_model(
    model: ModelRecord,
    backend: LoraTuningBackendKind,
) -> None:
    if model.capabilities and ModelCapability.CHAT not in model.capabilities:
        advertised = ", ".join(capability.value for capability in model.capabilities)
        raise ValueError(
            "LoRA tuning currently supports chat / causal-LM models only; "
            f"model `{model.model_ref}` advertises [{advertised}]"
        )

    if (
        backend == LoraTuningBackendKind.PEFT
        and model.primary_format != ModelFormat.SAFETENSORS
    ):
        raise ValueError(
            "PEFT LoRA tuning requires a safetensors chat model; "
            f"model `{model.model_ref}` has primary_format `{model.primary_format}`"
        )
    if (
        backend == LoraTuningBackendKind.MLX
        and model.primary_format != ModelFormat.MLX
    ):
        raise ValueError(
            "MLX LoRA tuning requires an MLX chat model; "
            f"model `{model.model_ref}` has primary_format `{model.primary_format}`"
        )


def normalize_lora_model_record(model: ModelRecord) -> ModelRecord:
    source_path = model.source_path.expanduser().resolve()
    if not source_path.exists():
        raise FileNotFoundError(f"LoRA model source `{source_path}` does not exist")
    if not source_path.is_dir():
        raise ValueError(f"LoRA model source `{source_path}` is not a directory")
    return replace(model, source_path=source_path)


def normalize_lora_dataset_config(config: LoraDatasetConfig) -> LoraDatasetConfig:
    source_path = config.source_path.expanduser().resolve()
    if not source_path.exists():
        raise FileNotFoundError(f"LoRA dataset source `{source_path}` does not exist")
    if not source_path.is_dir():
        raise ValueError(f"LoRA dataset source `{source_path}` is not a directory")
    train_path = source_path / "train.jsonl"
    if not train_path.exists():
        raise FileNotFoundError(f"LoRA dataset source `{source_path}` has no train.jsonl")
    if not train_path.is_file() or train_path.stat().st_size == 0:
        raise ValueError(f"LoRA dataset train split `{train_path}` must not be empty")
    if config.max_seq_length < 16 or config.max_seq_length > 131072:
        raise ValueError(
            "LoRA dataset max_seq_length must be between 16 and 131072; "
            f"got {config.max_seq_length}"
        )
    return replace(config, source_path=source_path)


def normalize_lora_config(config: LoraConfig) -> LoraConfig:
    if config.rank < 1 or config.rank > 1024:
        raise ValueError(f"LoRA rank must be between 1 and 1024; got {config.rank}")
    if config.alpha is not None and config.alpha < 1:
        raise ValueError(f"LoRA alpha must be positive; got {config.alpha}")
    if config.dropout != config.dropout or config.dropout < 0.0 or config.dropout > 1.0:
        raise ValueError(f"LoRA dropout must be between 0 and 1; got {config.dropout}")
    if config.scale != config.scale or config.scale <= 0.0:
        raise ValueError(f"MLX LoRA scale must be positive; got {config.scale}")
    target_modules = tuple(
        value.strip() for value in config.target_modules if value.strip()
    )
    return replace(config, target_modules=target_modules)


def normalize_lora_optimization_config(
    config: LoraOptimizationConfig,
) -> LoraOptimizationConfig:
    if config.max_steps < 1 or config.max_steps > 1_000_000:
        raise ValueError(
            f"LoRA max_steps must be between 1 and 1000000; got {config.max_steps}"
        )
    if config.batch_size < 1 or config.batch_size > 1024:
        raise ValueError(
            f"LoRA batch_size must be between 1 and 1024; got {config.batch_size}"
        )
    if config.learning_rate != config.learning_rate or config.learning_rate <= 0.0:
        raise ValueError(
            f"LoRA learning_rate must be positive; got {config.learning_rate}"
        )
    if config.weight_decay != config.weight_decay or config.weight_decay < 0.0:
        raise ValueError(
            f"LoRA weight_decay must be non-negative; got {config.weight_decay}"
        )
    if (
        config.gradient_accumulation_steps < 1
        or config.gradient_accumulation_steps > 1024
    ):
        raise ValueError(
            "LoRA gradient_accumulation_steps must be between 1 and 1024; "
            f"got {config.gradient_accumulation_steps}"
        )
    return config


def normalize_lora_checkpoint_config(
    config: LoraCheckpointConfig,
) -> LoraCheckpointConfig:
    if config.log_every_steps < 1:
        raise ValueError("LoRA log_every_steps must be positive")
    if config.eval_every_steps < 1:
        raise ValueError("LoRA eval_every_steps must be positive")
    if config.save_every_steps < 1:
        raise ValueError("LoRA save_every_steps must be positive")
    return config


def lora_plan_dict(request: LoraTuningRequest) -> dict[str, Any]:
    return {
        "model": {
            "model_ref": request.model.model_ref,
            "source_path": str(request.model.source_path),
            "primary_format": request.model.primary_format.value,
            "capabilities": sorted(
                capability.value for capability in request.model.capabilities
            ),
            "source_repo": request.model.source_repo,
            "source_revision": request.model.source_revision,
        },
        "dataset": {
            "source_path": str(request.dataset.source_path),
            "max_seq_length": request.dataset.max_seq_length,
            "mask_prompt": request.dataset.mask_prompt,
        },
        "lora": {
            "rank": request.lora.rank,
            "alpha": request.lora.alpha,
            "dropout": request.lora.dropout,
            "scale": request.lora.scale,
            "target_modules": list(request.lora.target_modules),
        },
        "optimization": {
            "max_steps": request.optimization.max_steps,
            "batch_size": request.optimization.batch_size,
            "learning_rate": request.optimization.learning_rate,
            "weight_decay": request.optimization.weight_decay,
            "gradient_accumulation_steps": (
                request.optimization.gradient_accumulation_steps
            ),
            "optimizer": request.optimization.optimizer,
            "seed": request.optimization.seed,
        },
        "checkpoint": {
            "log_every_steps": request.checkpoint.log_every_steps,
            "eval_every_steps": request.checkpoint.eval_every_steps,
            "save_every_steps": request.checkpoint.save_every_steps,
        },
        "backend_config": {
            "peft": request.backend_config.peft,
            "mlx": request.backend_config.mlx,
        },
    }
