from __future__ import annotations

import os
from abc import ABC
from typing import Any

from ..base import BackendConcurrencyPolicy, BackendFamily, BackendModel
from ..records import ModelFormat, ModelRecord


class TransformersBackendModel(BackendModel, ABC):
    """Base class for Hugging Face Transformers backend models."""

    family = BackendFamily.TRANSFORMERS
    concurrency_policy = BackendConcurrencyPolicy.EXCLUSIVE


def require_safetensors_model(record: ModelRecord, backend_name: str) -> None:
    if record.primary_format != ModelFormat.SAFETENSORS:
        raise ValueError(
            f"{backend_name} cannot load primary_format `{record.primary_format}`"
        )


def detect_torch_device(torch: Any, *, env_var: str | None = None) -> Any:
    requested = _requested_env_value(env_var)
    if requested:
        return requested_torch_device(torch, requested, env_var)

    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def pipeline_device(torch: Any, *, env_var: str | None = None) -> Any:
    device = detect_torch_device(torch, env_var=env_var)
    device_type = getattr(device, "type", str(device))
    if device_type == "cuda":
        return 0
    if device_type == "mps":
        return device
    return -1


def requested_torch_device(
    torch: Any,
    requested: str,
    env_var: str | None = None,
) -> Any:
    label = env_var or "torch device"
    if requested == "cpu":
        return torch.device("cpu")
    if requested == "cuda":
        if torch.cuda.is_available():
            return torch.device("cuda")
        raise RuntimeError(f"{label}=cuda was requested, but CUDA is not available")
    if requested == "mps":
        if torch.backends.mps.is_available():
            return torch.device("mps")
        raise RuntimeError(
            f"{label}=mps was requested, but PyTorch MPS is not available"
        )
    raise RuntimeError(
        f"unsupported {label} value `{requested}`; expected one of: cpu, mps, cuda"
    )


def move_batch_to_device(batch: dict[str, Any], device: Any) -> dict[str, Any]:
    return {
        key: value.to(device) if hasattr(value, "to") else value
        for key, value in batch.items()
    }


def load_transformers_component(component_class: Any, load_path: str) -> Any:
    return component_class.from_pretrained(load_path, trust_remote_code=True)


def load_transformers_model(model_class: Any, load_path: str, device: Any) -> Any:
    model = load_transformers_component(model_class, load_path)
    model.to(device)
    model.eval()
    return model


def clear_torch_device_cache(torch: Any) -> None:
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    if torch.backends.mps.is_available():
        torch.mps.empty_cache()


def _requested_env_value(env_var: str | None) -> str:
    if not env_var:
        return ""
    return os.environ.get(env_var, "").strip().lower()
