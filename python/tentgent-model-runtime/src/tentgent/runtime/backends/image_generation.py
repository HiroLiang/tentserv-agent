from __future__ import annotations

import json
from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import Any

from .base import BackendModel


DEFAULT_WIDTH = 512
DEFAULT_HEIGHT = 512
DEFAULT_STEPS = 20
DEFAULT_GUIDANCE_SCALE = 7.5
DEFAULT_TRANSFORM_STRENGTH = 0.6
DEFAULT_INPAINT_STRENGTH = 1.0
DEFAULT_CONTROL_KIND = "canny"
DEFAULT_CONTROL_STRENGTH = 1.0
MAX_PROMPT_BYTES = 8 * 1024


class ImageGenerationModelKind(StrEnum):
    DIFFUSERS_TEXT_TO_IMAGE = "diffusers-text-to-image"
    DIFFUSERS_IMAGE_TO_IMAGE = "diffusers-image-to-image"
    DIFFUSERS_INPAINT = "diffusers-inpaint"
    DIFFUSERS_CONTROL = "diffusers-control"
    MLX_DIFFUSION_TEXT_TO_IMAGE = "mlx-diffusion-text-to-image"
    MLX_DIFFUSION_IMAGE_TO_IMAGE = "mlx-diffusion-image-to-image"
    MLX_DIFFUSION_INPAINT = "mlx-diffusion-inpaint"


class ImageGenerationWorkflowKind(StrEnum):
    TEXT_TO_IMAGE = "text-to-image"
    IMAGE_TO_IMAGE = "image-to-image"
    INPAINT = "inpaint"
    CONTROL = "control"


class ImageGenerationOutputFormat(StrEnum):
    PNG = "png"
    JPG = "jpg"


@dataclass(frozen=True, slots=True)
class ImageGenerationAdapterSelection:
    adapter_ref: str
    source_path: Path
    lora_scale: float
    weight_file: str | None = None


@dataclass(frozen=True, slots=True)
class ImageGenerationControlSelection:
    control_ref: str
    source_path: Path
    control_kind: str


@dataclass(frozen=True, slots=True)
class ImageGenerationRequest:
    workflow_kind: ImageGenerationWorkflowKind
    prompt: str
    output_path: Path
    output_format: ImageGenerationOutputFormat
    input_image_path: Path | None = None
    input_image_media_type: str | None = None
    mask_image_path: Path | None = None
    mask_image_media_type: str | None = None
    strength: float | None = None
    control_image_path: Path | None = None
    control_image_media_type: str | None = None
    control_kind: str | None = None
    control_strength: float | None = None
    control: ImageGenerationControlSelection | None = None
    adapter: ImageGenerationAdapterSelection | None = None
    negative_prompt: str | None = None
    width: int = DEFAULT_WIDTH
    height: int = DEFAULT_HEIGHT
    steps: int = DEFAULT_STEPS
    guidance_scale: float = DEFAULT_GUIDANCE_SCALE
    seed: int | None = None


@dataclass(frozen=True, slots=True)
class ImageGenerationResult:
    output_format: ImageGenerationOutputFormat
    media_type: str
    output_path: Path
    total_bytes: int
    width: int
    height: int
    seed: int | None


class ImageGenerationBackendModel(BackendModel, ABC):
    @abstractmethod
    def generate_image(self, request: ImageGenerationRequest) -> ImageGenerationResult:
        """Run one image generation workflow."""
        raise NotImplementedError


ImageGenerationModelFactory = Callable[[Any], ImageGenerationBackendModel]


def build_image_generation_model(kind: Any) -> ImageGenerationBackendModel:
    try:
        image_kind = (
            kind
            if isinstance(kind, ImageGenerationModelKind)
            else ImageGenerationModelKind(kind)
        )
    except ValueError as exc:
        raise ValueError(f"unsupported image generation model kind `{kind}`") from exc

    if image_kind in {
        ImageGenerationModelKind.DIFFUSERS_TEXT_TO_IMAGE,
        ImageGenerationModelKind.DIFFUSERS_IMAGE_TO_IMAGE,
        ImageGenerationModelKind.DIFFUSERS_INPAINT,
        ImageGenerationModelKind.DIFFUSERS_CONTROL,
    }:
        from .diffusers import DiffusersImageGenerationModel

        return DiffusersImageGenerationModel()
    if image_kind in {
        ImageGenerationModelKind.MLX_DIFFUSION_TEXT_TO_IMAGE,
        ImageGenerationModelKind.MLX_DIFFUSION_IMAGE_TO_IMAGE,
        ImageGenerationModelKind.MLX_DIFFUSION_INPAINT,
    }:
        from .mlx import MfluxImageGenerationModel

        return MfluxImageGenerationModel()

    raise ValueError(f"unsupported image generation model kind `{kind}`")


def normalize_image_generation_output_format(
    value: str | ImageGenerationOutputFormat,
) -> ImageGenerationOutputFormat:
    if isinstance(value, ImageGenerationOutputFormat):
        return value
    normalized = value.strip().lower()
    if normalized == "jpeg":
        normalized = ImageGenerationOutputFormat.JPG.value
    try:
        return ImageGenerationOutputFormat(normalized)
    except ValueError as exc:
        expected = ", ".join(item.value for item in ImageGenerationOutputFormat)
        raise ValueError(
            f"unsupported image generation output format `{value}`; "
            f"expected one of: {expected}"
        ) from exc


def image_generation_media_type(
    output_format: str | ImageGenerationOutputFormat,
) -> str:
    normalized = normalize_image_generation_output_format(output_format)
    if normalized == ImageGenerationOutputFormat.JPG:
        return "image/jpeg"
    return "image/png"


def normalize_image_generation_request(
    request: ImageGenerationRequest,
) -> ImageGenerationRequest:
    prompt = _normalize_prompt(request.prompt, label="image generation prompt")
    negative_prompt = _normalize_optional_prompt(
        request.negative_prompt,
        label="image generation negative prompt",
    )
    output_path = request.output_path.expanduser().resolve()
    if output_path.exists():
        raise FileExistsError(f"image generation output path `{output_path}` already exists")

    output_format = normalize_image_generation_output_format(request.output_format)
    validate_image_generation_dimensions(request.width, request.height)
    validate_image_generation_steps(request.steps)
    validate_image_generation_guidance_scale(request.guidance_scale)

    input_image_path = _normalize_input_image_path(request.input_image_path)
    mask_image_path = _normalize_mask_image_path(request.mask_image_path)
    control_image_path = _normalize_control_image_path(request.control_image_path)
    strength = _normalize_denoise_strength(
        request.strength,
        input_image_path,
        mask_image_path,
        control_image_path,
    )
    control_kind = _normalize_control_kind(request.control_kind, control_image_path)
    control_strength = _normalize_control_strength(
        request.control_strength,
        control_image_path,
    )
    _validate_workflow_inputs(
        request.workflow_kind,
        input_image_path,
        mask_image_path,
        control_image_path,
    )

    adapter = _normalize_adapter(request.adapter)
    control = _normalize_control(request.control, control_image_path, control_kind)

    return ImageGenerationRequest(
        workflow_kind=request.workflow_kind,
        prompt=prompt,
        negative_prompt=negative_prompt,
        output_path=output_path,
        output_format=output_format,
        input_image_path=input_image_path,
        input_image_media_type=normalize_input_image_media_type(
            request.input_image_media_type
        ),
        mask_image_path=mask_image_path,
        mask_image_media_type=normalize_input_image_media_type(
            request.mask_image_media_type
        ),
        strength=strength,
        control_image_path=control_image_path,
        control_image_media_type=normalize_input_image_media_type(
            request.control_image_media_type
        ),
        control_kind=control_kind,
        control_strength=control_strength,
        control=control,
        adapter=adapter,
        width=request.width,
        height=request.height,
        steps=request.steps,
        guidance_scale=request.guidance_scale,
        seed=request.seed,
    )


def validate_image_generation_dimensions(width: int, height: int) -> None:
    _validate_side("width", width)
    _validate_side("height", height)


def validate_image_generation_steps(steps: int) -> None:
    if steps < 1 or steps > 100:
        raise ValueError(f"image generation steps must be between 1 and 100; got {steps}")


def validate_image_generation_guidance_scale(guidance_scale: float) -> None:
    if guidance_scale != guidance_scale or guidance_scale < 0.0 or guidance_scale > 30.0:
        raise ValueError(
            "image generation guidance scale must be between 0 and 30; "
            f"got {guidance_scale}"
        )


def validate_lora_scale(lora_scale: float) -> None:
    if lora_scale != lora_scale or lora_scale < 0.0 or lora_scale > 4.0:
        raise ValueError(f"image LoRA scale must be between 0 and 4; got {lora_scale}")


def normalize_input_image_media_type(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.strip().lower().split(";")[0]
    return normalized or None


def write_image_generation_output(
    request: ImageGenerationRequest,
    image: object,
) -> ImageGenerationResult:
    request.output_path.parent.mkdir(parents=True, exist_ok=True)
    if request.output_format == ImageGenerationOutputFormat.JPG and hasattr(
        image,
        "convert",
    ):
        image = image.convert("RGB")
    save_kwargs: dict[str, object] = {}
    if request.output_format == ImageGenerationOutputFormat.JPG:
        save_kwargs["quality"] = 95
    image.save(request.output_path, **save_kwargs)
    return ImageGenerationResult(
        output_format=normalize_image_generation_output_format(request.output_format),
        media_type=image_generation_media_type(request.output_format),
        output_path=request.output_path,
        total_bytes=request.output_path.stat().st_size,
        width=request.width,
        height=request.height,
        seed=request.seed,
    )


def load_normalized_inpaint_images(
    image_path: Path,
    mask_path: Path,
    width: int,
    height: int,
) -> tuple[Any, Any]:
    image_module = _load_pillow_image()
    with image_module.open(image_path) as image:
        base_image = image.convert("RGB")
        if base_image.size != (width, height):
            base_image = base_image.resize((width, height))
    with image_module.open(mask_path) as mask:
        mask_image = normalize_inpaint_mask(mask, width, height)
    return base_image, mask_image


def load_normalized_control_image(
    image_path: Path,
    width: int,
    height: int,
) -> Any:
    image_module = _load_pillow_image()
    with image_module.open(image_path) as image:
        control_image = image.convert("RGB")
        if control_image.size != (width, height):
            control_image = control_image.resize((width, height))
        return control_image


def normalize_inpaint_mask(mask: Any, width: int, height: int) -> Any:
    mask_image = mask.convert("L")
    if mask_image.size != (width, height):
        mask_image = mask_image.resize((width, height))
    return mask_image.point(lambda value: 255 if value >= 128 else 0)


def _normalize_prompt(value: str, *, label: str) -> str:
    prompt = value.strip()
    if not prompt:
        raise ValueError(f"{label} must not be empty")
    if len(prompt.encode("utf-8")) > MAX_PROMPT_BYTES:
        raise ValueError(f"{label} must be at most {MAX_PROMPT_BYTES} bytes")
    return prompt


def _normalize_optional_prompt(value: str | None, *, label: str) -> str | None:
    if value is None:
        return None
    prompt = value.strip()
    if not prompt:
        return None
    if len(prompt.encode("utf-8")) > MAX_PROMPT_BYTES:
        raise ValueError(f"{label} must be at most {MAX_PROMPT_BYTES} bytes")
    return prompt


def _normalize_input_image_path(input_image_path: Path | None) -> Path | None:
    if input_image_path is None:
        return None
    path = input_image_path.expanduser().resolve()
    if not path.exists():
        raise FileNotFoundError(f"image transform input image `{path}` does not exist")
    if not path.is_file():
        raise ValueError(f"image transform input image `{path}` is not a file")
    if path.stat().st_size == 0:
        raise ValueError(f"image transform input image `{path}` must not be empty")
    return path


def _normalize_mask_image_path(mask_image_path: Path | None) -> Path | None:
    if mask_image_path is None:
        return None
    path = mask_image_path.expanduser().resolve()
    if not path.exists():
        raise FileNotFoundError(f"image inpaint mask image `{path}` does not exist")
    if not path.is_file():
        raise ValueError(f"image inpaint mask image `{path}` is not a file")
    if path.stat().st_size == 0:
        raise ValueError(f"image inpaint mask image `{path}` must not be empty")
    return path


def _normalize_control_image_path(control_image_path: Path | None) -> Path | None:
    if control_image_path is None:
        return None
    path = control_image_path.expanduser().resolve()
    if not path.exists():
        raise FileNotFoundError(f"image control input image `{path}` does not exist")
    if not path.is_file():
        raise ValueError(f"image control input image `{path}` is not a file")
    if path.stat().st_size == 0:
        raise ValueError(f"image control input image `{path}` must not be empty")
    return path


def _normalize_denoise_strength(
    strength: float | None,
    input_image_path: Path | None,
    mask_image_path: Path | None,
    control_image_path: Path | None,
) -> float | None:
    if control_image_path is not None:
        if strength is not None:
            raise ValueError("image denoising strength cannot be used with image control")
        return None
    if input_image_path is None:
        if mask_image_path is not None:
            raise ValueError("image inpaint mask requires input_image_path")
        if strength is not None:
            raise ValueError("image denoising strength requires input_image_path")
        return None

    if strength is None:
        strength = (
            DEFAULT_INPAINT_STRENGTH
            if mask_image_path is not None
            else DEFAULT_TRANSFORM_STRENGTH
        )
    if strength != strength or strength < 0.0 or strength > 1.0:
        raise ValueError(f"image denoising strength must be between 0 and 1; got {strength}")
    return strength


def _normalize_control_kind(
    control_kind: str | None,
    control_image_path: Path | None,
) -> str | None:
    if control_image_path is None:
        if control_kind is not None:
            raise ValueError("image control kind requires control_image_path")
        return None
    normalized = (control_kind or DEFAULT_CONTROL_KIND).strip().lower()
    if normalized != "canny":
        raise ValueError(
            f"unsupported image control kind `{control_kind}`; expected one of: canny"
        )
    return normalized


def _normalize_control_strength(
    control_strength: float | None,
    control_image_path: Path | None,
) -> float | None:
    if control_image_path is None:
        if control_strength is not None:
            raise ValueError("image control strength requires control_image_path")
        return None
    value = DEFAULT_CONTROL_STRENGTH if control_strength is None else control_strength
    if value != value or value < 0.0 or value > 2.0:
        raise ValueError(f"image control strength must be between 0 and 2; got {value}")
    return value


def _validate_workflow_inputs(
    workflow_kind: ImageGenerationWorkflowKind,
    input_image_path: Path | None,
    mask_image_path: Path | None,
    control_image_path: Path | None,
) -> None:
    if workflow_kind == ImageGenerationWorkflowKind.TEXT_TO_IMAGE:
        if input_image_path or mask_image_path or control_image_path:
            raise ValueError("text-to-image cannot include image input paths")
        return
    if workflow_kind == ImageGenerationWorkflowKind.IMAGE_TO_IMAGE:
        if input_image_path is None:
            raise ValueError("image-to-image requires input_image_path")
        if mask_image_path or control_image_path:
            raise ValueError("image-to-image cannot include mask or control paths")
        return
    if workflow_kind == ImageGenerationWorkflowKind.INPAINT:
        if input_image_path is None or mask_image_path is None:
            raise ValueError("inpaint requires input_image_path and mask_image_path")
        if control_image_path:
            raise ValueError("inpaint cannot include control_image_path")
        _validate_inpaint_sizes(input_image_path, mask_image_path)
        return
    if workflow_kind == ImageGenerationWorkflowKind.CONTROL:
        if control_image_path is None:
            raise ValueError("control requires control_image_path")
        if input_image_path or mask_image_path:
            raise ValueError("control cannot include image-to-image or inpaint paths")
        return
    raise ValueError(f"unsupported image workflow kind `{workflow_kind}`")


def _validate_inpaint_sizes(input_image_path: Path, mask_image_path: Path) -> None:
    image = _load_pillow_image()
    with image.open(input_image_path) as input_image:
        input_size = input_image.size
    with image.open(mask_image_path) as mask_image:
        mask_size = mask_image.size
    if input_size != mask_size:
        raise ValueError(
            "image inpaint input image and mask image must have matching dimensions; "
            f"got image {input_size[0]}x{input_size[1]} and "
            f"mask {mask_size[0]}x{mask_size[1]}"
        )


def _normalize_adapter(
    adapter: ImageGenerationAdapterSelection | None,
) -> ImageGenerationAdapterSelection | None:
    if adapter is None:
        return None
    validate_lora_scale(adapter.lora_scale)
    source_path = adapter.source_path.expanduser().resolve()
    if not source_path.exists():
        raise FileNotFoundError(f"image LoRA adapter source `{source_path}` does not exist")
    return ImageGenerationAdapterSelection(
        adapter_ref=adapter.adapter_ref,
        source_path=source_path,
        lora_scale=adapter.lora_scale,
        weight_file=adapter.weight_file,
    )


def _normalize_control(
    control: ImageGenerationControlSelection | None,
    control_image_path: Path | None,
    control_kind: str | None,
) -> ImageGenerationControlSelection | None:
    if control is None:
        if control_image_path is not None:
            raise ValueError("image control input requires control")
        return None
    if control_image_path is None:
        raise ValueError("image control adapter requires control_image_path")
    source_path = control.source_path.expanduser().resolve()
    if not source_path.exists():
        raise FileNotFoundError(f"image control adapter source `{source_path}` does not exist")
    if control.control_kind != control_kind:
        raise ValueError(
            f"image control adapter kind `{control.control_kind}` "
            f"does not match request `{control_kind}`"
        )
    return ImageGenerationControlSelection(
        control_ref=control.control_ref,
        source_path=source_path,
        control_kind=control.control_kind,
    )


def _load_pillow_image() -> Any:
    try:
        from PIL import Image
    except ModuleNotFoundError as exc:
        if exc.name == "PIL":
            from .errors import missing_backend_dependency

            raise missing_backend_dependency(exc.name) from exc
        raise
    return Image


def _validate_side(axis: str, value: int) -> None:
    if value < 64 or value > 1024:
        raise ValueError(
            f"image generation {axis} must be between 64 and 1024 pixels; got {value}"
        )
    if value % 8 != 0:
        raise ValueError(
            f"image generation {axis} must be divisible by 8 pixels; got {value}"
        )


def declares_missing_safety_checker(model_path: Path) -> bool:
    model_index_path = model_path / "model_index.json"
    try:
        raw = json.loads(model_index_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False

    return "safety_checker" in raw and not (model_path / "safety_checker").exists()
