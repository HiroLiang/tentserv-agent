from __future__ import annotations

import random
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any

from .base import ImageGenerationBackend
from ..runtime.image_generation import (
    ImageGenerationAdapterSelection,
    ImageGenerationRequest,
    ImageGenerationResult,
    image_generation_media_type,
    load_normalized_inpaint_images,
    normalize_image_generation_output_format,
)
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.records import StoredModelRecord


@dataclass(frozen=True)
class MfluxDeps:
    Flux1: Any
    Flux1Fill: Any
    ModelConfig: Any


class MfluxImageGenerationBackend(ImageGenerationBackend):
    def __init__(self) -> None:
        self._deps = _load_mflux_deps()
        self._record: StoredModelRecord | None = None
        self._model: Any | None = None
        self._model_workflow: str | None = None
        self._adapter: ImageGenerationAdapterSelection | None = None

    def load(self, record: StoredModelRecord) -> None:
        if not record.source_repo:
            raise RuntimeError(
                "MFLUX image generation requires Hugging Face source repo metadata "
                "so Tentgent can select the matching base model family."
            )
        self._record = record
        self._model = None
        self._model_workflow = None

    def _load_model(self, workflow: str) -> Any:
        if self._record is None:
            raise RuntimeError(
                "MFLUX image generation backend is not loaded yet; "
                "call load() before generate_image()."
            )
        if self._model is not None and self._model_workflow == workflow:
            return self._model
        if workflow == "inpaint":
            _ensure_mflux_fill_model(self._record)

        model_config = _mflux_flux_model_config(self._record, self._deps.ModelConfig)
        quantize = _mflux_quantize_bits(self._record)
        lora_paths = None
        lora_scales = None
        if self._adapter is not None:
            lora_paths = [str(_adapter_weight_path(self._adapter))]
            lora_scales = [self._adapter.lora_scale]
        model_class = self._deps.Flux1Fill if workflow == "inpaint" else self._deps.Flux1
        self._model = model_class(
            model_config=model_config,
            quantize=quantize,
            model_path=str(self._record.variant_source_path),
            lora_paths=lora_paths,
            lora_scales=lora_scales,
        )
        self._model_workflow = workflow
        return self._model

    def select_adapter(self, adapter: ImageGenerationAdapterSelection | None) -> None:
        if self._model is not None:
            raise RuntimeError(
                "MFLUX image generation requires selecting LoRA adapters before load()."
            )
        self._adapter = adapter

    def generate_image(self, request: ImageGenerationRequest) -> ImageGenerationResult:
        if request.control_image_path is not None:
            raise RuntimeError(
                "MFLUX image control / ControlNet generation is not supported in this slice."
            )
        workflow = _workflow_for_request(request)
        model = self._load_model(workflow)
        seed = request.seed
        if seed is None:
            seed = random.SystemRandom().randrange(0, 2**63)
        output_request = replace(request, seed=seed)
        if workflow == "inpaint":
            if request.negative_prompt:
                raise RuntimeError(
                    "MFLUX inpainting does not support negative prompts in this slice."
                )
            mask_path = _write_mflux_normalized_mask(request)
            try:
                generated = model.generate_image(
                    seed=seed,
                    prompt=request.prompt,
                    image_path=_required_path(request.input_image_path, "input image"),
                    masked_image_path=mask_path,
                    width=request.width,
                    height=request.height,
                    num_inference_steps=request.steps,
                    guidance=request.guidance_scale,
                    image_strength=_mflux_image_strength(request),
                )
            finally:
                mask_path.unlink(missing_ok=True)
        else:
            generated = model.generate_image(
                seed=seed,
                prompt=request.prompt,
                width=request.width,
                height=request.height,
                num_inference_steps=request.steps,
                guidance=request.guidance_scale,
                negative_prompt=request.negative_prompt,
                image_path=request.input_image_path,
                image_strength=_mflux_image_strength(request),
            )
        return _write_mflux_image_output(output_request, generated)

    def release(self) -> None:
        self._record = None
        self._model = None
        self._model_workflow = None
        self._adapter = None
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


def _adapter_weight_path(adapter: ImageGenerationAdapterSelection) -> Any:
    source_path = adapter.source_path
    if adapter.weight_file:
        return source_path / adapter.weight_file
    if source_path.is_file():
        return source_path
    candidates = sorted(source_path.rglob("*.safetensors"))
    if len(candidates) == 1:
        return candidates[0]
    if not candidates:
        raise FileNotFoundError(
            f"image LoRA adapter `{adapter.adapter_ref[:12]}` has no .safetensors weights"
        )
    names = ", ".join(str(path.relative_to(source_path)) for path in candidates)
    raise ValueError(
        f"image LoRA adapter `{adapter.adapter_ref[:12]}` has multiple .safetensors "
        f"weights; select one in adapter metadata. Candidates: {names}"
    )


def _load_mflux_deps() -> MfluxDeps:
    try:
        from mflux.models.common.config import ModelConfig
        from mflux.models.flux.variants.fill.flux_fill import Flux1Fill
        from mflux.models.flux.variants.txt2img.flux import Flux1
    except ModuleNotFoundError as exc:
        if exc.name == "mflux":
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise
    except ImportError as exc:
        raise RuntimeError(
            "MFLUX image generation requires a recent local-model runtime with "
            "Flux text-to-image and Flux Fill API support. Run "
            "`tentgent runtime bootstrap --profile local-model`."
        ) from exc

    return MfluxDeps(Flux1=Flux1, Flux1Fill=Flux1Fill, ModelConfig=ModelConfig)


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


def _workflow_for_request(request: ImageGenerationRequest) -> str:
    if request.mask_image_path is not None:
        return "inpaint"
    if request.input_image_path is not None:
        return "image-to-image"
    return "text-to-image"


def _ensure_mflux_fill_model(record: StoredModelRecord) -> None:
    label = " ".join(
        value
        for value in [record.source_repo, record.source_path, str(record.variant_source_path)]
        if value
    ).lower()
    if "fill" not in label and "inpaint" not in label:
        raise RuntimeError(
            "MFLUX inpainting requires a Flux Fill-compatible model; "
            f"model `{record.source_repo or record.short_ref}` does not look like a fill model."
        )


def _mflux_image_strength(request: ImageGenerationRequest) -> float | None:
    if request.input_image_path is None:
        return None
    if request.strength is None:
        raise ValueError("image denoising strength is required")
    return 1.0 - request.strength


def _required_path(path: Path | None, label: str) -> Path:
    if path is None:
        raise ValueError(f"MFLUX inpainting requires {label}")
    return path


def _write_mflux_normalized_mask(request: ImageGenerationRequest) -> Path:
    input_path = _required_path(request.input_image_path, "input image")
    mask_path = _required_path(request.mask_image_path, "mask image")
    _, mask = load_normalized_inpaint_images(
        input_path,
        mask_path,
        request.width,
        request.height,
    )
    normalized_path = request.output_path.with_name(
        f".{request.output_path.name}.tentgent-mask.png"
    )
    normalized_path.parent.mkdir(parents=True, exist_ok=True)
    mask.save(normalized_path)
    return normalized_path


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
