from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Any

from .base import ImageGenerationBackend
from ..runtime.image_generation import (
    ImageGenerationRequest,
    ImageGenerationResult,
    write_image_generation_output,
)
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.records import StoredModelRecord


@dataclass(frozen=True)
class DiffusersDeps:
    torch: Any
    DiffusionPipeline: Any


class DiffusersImageGenerationBackend(ImageGenerationBackend):
    def __init__(self) -> None:
        self._deps = _load_diffusers_deps()
        self._record: StoredModelRecord | None = None
        self._pipeline: Any | None = None
        self._device = _detect_device(self._deps.torch)

    def load(self, record: StoredModelRecord) -> None:
        load_kwargs = _diffusers_load_kwargs(record, self._deps.torch, self._device)
        pipeline = self._deps.DiffusionPipeline.from_pretrained(
            str(record.variant_source_path),
            local_files_only=True,
            **load_kwargs,
        )
        pipeline.to(self._device)
        if hasattr(pipeline, "enable_attention_slicing"):
            pipeline.enable_attention_slicing()

        self._record = record
        self._pipeline = pipeline

    def generate_image(self, request: ImageGenerationRequest) -> ImageGenerationResult:
        pipeline = self._require_loaded()
        kwargs: dict[str, object] = {
            "prompt": request.prompt,
            "width": request.width,
            "height": request.height,
            "num_inference_steps": request.steps,
            "guidance_scale": request.guidance_scale,
        }
        if request.negative_prompt:
            kwargs["negative_prompt"] = request.negative_prompt
        if request.seed is not None:
            kwargs["generator"] = self._deps.torch.Generator(device="cpu").manual_seed(
                request.seed
            )

        with self._deps.torch.inference_mode():
            raw_result = pipeline(**kwargs)
        image = raw_result.images[0]
        return write_image_generation_output(request, image)

    def release(self) -> None:
        self._record = None
        self._pipeline = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> Any:
        if self._record is None or self._pipeline is None:
            raise RuntimeError(
                "Diffusers image generation backend is not loaded yet; "
                "call load() before generate_image()."
            )
        return self._pipeline


def _load_diffusers_deps() -> DiffusersDeps:
    try:
        import torch
        from diffusers import DiffusionPipeline
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "diffusers"}:
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise

    return DiffusersDeps(torch=torch, DiffusionPipeline=DiffusionPipeline)


def _diffusers_load_kwargs(
    record: StoredModelRecord, torch: Any, device: Any
) -> dict[str, object]:
    kwargs: dict[str, object] = {
        "torch_dtype": _torch_dtype_for_device(torch, device),
    }
    if _declares_missing_safety_checker(record):
        kwargs["safety_checker"] = None
    return kwargs


def _declares_missing_safety_checker(record: StoredModelRecord) -> bool:
    model_index_path = record.variant_source_path / "model_index.json"
    try:
        raw = json.loads(model_index_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False

    return "safety_checker" in raw and not (
        record.variant_source_path / "safety_checker"
    ).exists()


def _detect_device(torch: Any) -> Any:
    requested = os.environ.get("TENTGENT_IMAGE_GENERATION_DEVICE", "").strip().lower()
    if requested:
        return _requested_device(torch, requested)

    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _requested_device(torch: Any, requested: str) -> Any:
    if requested == "cpu":
        return torch.device("cpu")
    if requested == "cuda":
        if torch.cuda.is_available():
            return torch.device("cuda")
        raise RuntimeError(
            "TENTGENT_IMAGE_GENERATION_DEVICE=cuda was requested, but CUDA is not available"
        )
    if requested == "mps":
        if torch.backends.mps.is_available():
            return torch.device("mps")
        raise RuntimeError(
            "TENTGENT_IMAGE_GENERATION_DEVICE=mps was requested, but PyTorch MPS is not available"
        )
    raise RuntimeError(
        "unsupported TENTGENT_IMAGE_GENERATION_DEVICE value "
        f"`{requested}`; expected one of: cpu, mps, cuda"
    )


def _torch_dtype_for_device(torch: Any, device: Any) -> Any:
    requested = os.environ.get("TENTGENT_IMAGE_GENERATION_TORCH_DTYPE", "").strip().lower()
    if requested:
        return _requested_torch_dtype(torch, requested)

    device_type = getattr(device, "type", str(device))
    if device_type == "cuda":
        return torch.float16
    return torch.float32


def _requested_torch_dtype(torch: Any, requested: str) -> Any:
    if requested in {"float32", "fp32"}:
        return torch.float32
    if requested in {"float16", "fp16"}:
        return torch.float16
    raise RuntimeError(
        "unsupported TENTGENT_IMAGE_GENERATION_TORCH_DTYPE value "
        f"`{requested}`; expected one of: float32, float16"
    )
