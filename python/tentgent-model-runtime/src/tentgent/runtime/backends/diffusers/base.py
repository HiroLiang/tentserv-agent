from __future__ import annotations

import os
from abc import ABC
from typing import Any

from ..base import BackendConcurrencyPolicy, BackendFamily, BackendModel
from ..image_generation import declares_missing_safety_checker
from ..records import ModelFormat, ModelRecord


IMAGE_GENERATION_DEVICE_ENV = "TENTGENT_IMAGE_GENERATION_DEVICE"
IMAGE_GENERATION_TORCH_DTYPE_ENV = "TENTGENT_IMAGE_GENERATION_TORCH_DTYPE"


class DiffusersBackendModel(BackendModel, ABC):
    """Base class for Hugging Face Diffusers backend models."""

    family = BackendFamily.DIFFUSERS
    concurrency_policy = BackendConcurrencyPolicy.EXCLUSIVE


def require_diffusers_model(record: ModelRecord, backend_name: str) -> None:
    if record.primary_format != ModelFormat.DIFFUSERS:
        raise ValueError(
            f"{backend_name} cannot load primary_format `{record.primary_format}`"
        )


def detect_diffusers_device(torch: Any) -> Any:
    requested = os.environ.get(IMAGE_GENERATION_DEVICE_ENV, "").strip().lower()
    if requested:
        return requested_torch_device(torch, requested, IMAGE_GENERATION_DEVICE_ENV)

    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def diffusers_load_kwargs(
    record: ModelRecord,
    torch: Any,
    device: Any,
) -> dict[str, object]:
    kwargs: dict[str, object] = {
        "torch_dtype": torch_dtype_for_device(torch, device),
    }
    if declares_missing_safety_checker(record.source_path):
        kwargs["safety_checker"] = None
    return kwargs


def controlnet_load_kwargs(torch: Any, device: Any) -> dict[str, object]:
    return {
        "torch_dtype": torch_dtype_for_device(torch, device),
    }


def torch_dtype_for_device(torch: Any, device: Any) -> Any:
    requested = os.environ.get(IMAGE_GENERATION_TORCH_DTYPE_ENV, "").strip().lower()
    if requested:
        return requested_torch_dtype(torch, requested)

    device_type = getattr(device, "type", str(device))
    if device_type == "cuda":
        return torch.float16
    return torch.float32


def requested_torch_device(torch: Any, requested: str, env_var: str) -> Any:
    if requested == "cpu":
        return torch.device("cpu")
    if requested == "cuda":
        if torch.cuda.is_available():
            return torch.device("cuda")
        raise RuntimeError(f"{env_var}=cuda was requested, but CUDA is not available")
    if requested == "mps":
        if torch.backends.mps.is_available():
            return torch.device("mps")
        raise RuntimeError(
            f"{env_var}=mps was requested, but PyTorch MPS is not available"
        )
    raise RuntimeError(
        f"unsupported {env_var} value `{requested}`; "
        "expected one of: cpu, mps, cuda"
    )


def requested_torch_dtype(torch: Any, requested: str) -> Any:
    if requested in {"float32", "fp32"}:
        return torch.float32
    if requested in {"float16", "fp16"}:
        return torch.float16
    raise RuntimeError(
        f"unsupported {IMAGE_GENERATION_TORCH_DTYPE_ENV} value `{requested}`; "
        "expected one of: float32, float16"
    )


def clear_torch_device_cache(torch: Any) -> None:
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    if torch.backends.mps.is_available():
        torch.mps.empty_cache()
