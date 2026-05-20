from __future__ import annotations

import random
from dataclasses import dataclass, replace
from typing import Any

from .base import ImageGenerationBackend
from ..runtime.image_generation import (
    ImageGenerationRequest,
    ImageGenerationResult,
    image_generation_media_type,
    normalize_image_generation_output_format,
)
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.records import StoredModelRecord


@dataclass(frozen=True)
class MfluxDeps:
    Flux1: Any
    ModelConfig: Any


class MfluxImageGenerationBackend(ImageGenerationBackend):
    def __init__(self) -> None:
        self._deps = _load_mflux_deps()
        self._record: StoredModelRecord | None = None
        self._model: Any | None = None

    def load(self, record: StoredModelRecord) -> None:
        if not record.source_repo:
            raise RuntimeError(
                "MFLUX image generation requires Hugging Face source repo metadata "
                "so Tentgent can select the matching base model family."
            )
        model_config = _mflux_flux_model_config(record, self._deps.ModelConfig)
        quantize = _mflux_quantize_bits(record)
        self._model = self._deps.Flux1(
            model_config=model_config,
            quantize=quantize,
            model_path=str(record.variant_source_path),
        )
        self._record = record

    def generate_image(self, request: ImageGenerationRequest) -> ImageGenerationResult:
        model = self._require_loaded()
        seed = request.seed
        if seed is None:
            seed = random.SystemRandom().randrange(0, 2**63)
        output_request = replace(request, seed=seed)
        generated = model.generate_image(
            seed=seed,
            prompt=request.prompt,
            width=request.width,
            height=request.height,
            num_inference_steps=request.steps,
            guidance=request.guidance_scale,
            negative_prompt=request.negative_prompt,
        )
        return _write_mflux_image_output(output_request, generated)

    def release(self) -> None:
        self._record = None
        self._model = None
        try:
            import mlx.core as mx

            metal = getattr(mx, "metal", None)
            if metal is not None and hasattr(metal, "clear_cache"):
                metal.clear_cache()
        except ModuleNotFoundError:
            return

    def _require_loaded(self) -> Any:
        if self._record is None or self._model is None:
            raise RuntimeError(
                "MFLUX image generation backend is not loaded yet; "
                "call load() before generate_image()."
            )
        return self._model


def _load_mflux_deps() -> MfluxDeps:
    try:
        from mflux.models.common.config import ModelConfig
        from mflux.models.flux.variants.txt2img.flux import Flux1
    except ModuleNotFoundError as exc:
        if exc.name == "mflux":
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise

    return MfluxDeps(Flux1=Flux1, ModelConfig=ModelConfig)


def _mflux_flux_model_config(record: StoredModelRecord, model_config: Any) -> Any:
    source_repo = record.source_repo
    if not source_repo:
        raise RuntimeError("missing source repo metadata for MFLUX image generation")
    base_model = _mflux_base_model(source_repo)
    try:
        return model_config.from_name(model_name=source_repo, base_model=base_model)
    except Exception as exc:  # noqa: BLE001 - runtime packages raise custom errors.
        raise RuntimeError(
            "MFLUX image generation supports Flux-family MLX models in this slice, "
            f"but model `{source_repo}` could not be mapped to a Flux base model: {exc}"
        ) from exc


def _mflux_base_model(source_repo: str) -> str | None:
    lowered = source_repo.lower()
    if "schnell" in lowered or "lite" in lowered:
        return "schnell"
    if "dev" in lowered:
        return "dev"
    return None


def _mflux_quantize_bits(record: StoredModelRecord) -> int | None:
    label = " ".join(
        value
        for value in [record.source_repo, record.source_path, str(record.variant_source_path)]
        if value
    ).lower()
    if "q8" in label or "8bit" in label or "8-bit" in label:
        return 8
    if "q4" in label or "4bit" in label or "4-bit" in label:
        return 4
    return None


def _write_mflux_image_output(
    request: ImageGenerationRequest,
    image: Any,
) -> ImageGenerationResult:
    output_format = normalize_image_generation_output_format(request.output_format)
    request.output_path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path=request.output_path)
    return ImageGenerationResult(
        output_format=output_format,
        media_type=image_generation_media_type(output_format),
        output_path=request.output_path,
        total_bytes=request.output_path.stat().st_size,
        width=request.width,
        height=request.height,
        seed=request.seed,
    )
