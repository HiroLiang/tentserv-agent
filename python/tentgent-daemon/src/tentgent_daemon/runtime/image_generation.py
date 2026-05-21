from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_image_generation_backend


PNG_FORMAT = "png"
JPG_FORMAT = "jpg"
SUPPORTED_OUTPUT_FORMATS = {PNG_FORMAT, JPG_FORMAT}

DEFAULT_WIDTH = 512
DEFAULT_HEIGHT = 512
DEFAULT_STEPS = 20
DEFAULT_GUIDANCE_SCALE = 7.5
DEFAULT_TRANSFORM_STRENGTH = 0.6
MAX_PROMPT_BYTES = 8 * 1024


@dataclass(frozen=True)
class ImageGenerationAdapterSelection:
    adapter_ref: str
    source_path: Path
    lora_scale: float
    weight_file: str | None = None


@dataclass(frozen=True)
class ImageGenerationRequest:
    model_ref: str
    prompt: str
    output_path: Path
    output_format: str
    input_image_path: Path | None = None
    input_image_media_type: str | None = None
    strength: float | None = None
    adapter: ImageGenerationAdapterSelection | None = None
    negative_prompt: str | None = None
    width: int = DEFAULT_WIDTH
    height: int = DEFAULT_HEIGHT
    steps: int = DEFAULT_STEPS
    guidance_scale: float = DEFAULT_GUIDANCE_SCALE
    seed: int | None = None


@dataclass(frozen=True)
class ImageGenerationResult:
    output_format: str
    media_type: str
    output_path: Path
    total_bytes: int
    width: int
    height: int
    seed: int | None


@dataclass(frozen=True)
class ImageGenerationPlan:
    request: ImageGenerationRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_image_generation_plan(
    request: ImageGenerationRequest,
    home: Path | None = None,
) -> ImageGenerationPlan:
    record = load_model_record(request.model_ref, home=home)
    if "image-generation" not in record.model_capabilities:
        capabilities = ", ".join(record.model_capabilities)
        raise ValueError(
            "image generation endpoint requires model capability "
            f"`image-generation`, but model `{record.model_ref}` advertises "
            f"[{capabilities}]"
        )

    prompt = request.prompt.strip()
    if not prompt:
        raise ValueError("image generation prompt must not be empty")
    if len(prompt.encode("utf-8")) > MAX_PROMPT_BYTES:
        raise ValueError(
            f"image generation prompt must be at most {MAX_PROMPT_BYTES} bytes"
        )

    negative_prompt = request.negative_prompt.strip() if request.negative_prompt else None
    if negative_prompt == "":
        negative_prompt = None
    if negative_prompt and len(negative_prompt.encode("utf-8")) > MAX_PROMPT_BYTES:
        raise ValueError(
            f"image generation negative prompt must be at most {MAX_PROMPT_BYTES} bytes"
        )

    output_path = request.output_path.expanduser().resolve()
    if output_path.exists():
        raise FileExistsError(f"image generation output path `{output_path}` already exists")

    output_format = normalize_image_generation_output_format(request.output_format)
    validate_image_generation_dimensions(request.width, request.height)
    validate_image_generation_steps(request.steps)
    validate_image_generation_guidance_scale(request.guidance_scale)
    input_image_path = normalize_input_image_path(request.input_image_path)
    strength = normalize_transform_strength(request.strength, input_image_path)
    if request.adapter is not None:
        validate_lora_scale(request.adapter.lora_scale)
        if not request.adapter.source_path.exists():
            raise FileNotFoundError(
                f"image LoRA adapter source `{request.adapter.source_path}` does not exist"
            )

    return ImageGenerationPlan(
        request=ImageGenerationRequest(
            model_ref=request.model_ref,
            prompt=prompt,
            negative_prompt=negative_prompt,
            output_path=output_path,
            output_format=output_format,
            input_image_path=input_image_path,
            input_image_media_type=normalize_input_image_media_type(
                request.input_image_media_type
            ),
            strength=strength,
            adapter=request.adapter,
            width=request.width,
            height=request.height,
            steps=request.steps,
            guidance_scale=request.guidance_scale,
            seed=request.seed,
        ),
        record=record,
        backend=resolve_image_generation_backend(record),
        load_path=record.variant_source_path,
    )


def normalize_image_generation_output_format(value: str) -> str:
    normalized = value.strip().lower()
    if normalized == "jpeg":
        normalized = JPG_FORMAT
    if normalized not in SUPPORTED_OUTPUT_FORMATS:
        expected = ", ".join(sorted(SUPPORTED_OUTPUT_FORMATS))
        raise ValueError(
            f"unsupported image generation output format `{value}`; "
            f"expected one of: {expected}"
        )
    return normalized


def image_generation_media_type(output_format: str) -> str:
    output_format = normalize_image_generation_output_format(output_format)
    if output_format == JPG_FORMAT:
        return "image/jpeg"
    return "image/png"


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


def normalize_input_image_path(input_image_path: Path | None) -> Path | None:
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


def normalize_input_image_media_type(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.strip().lower().split(";")[0]
    return normalized or None


def normalize_transform_strength(
    strength: float | None,
    input_image_path: Path | None,
) -> float | None:
    if input_image_path is None:
        if strength is not None:
            raise ValueError(
                "image transform strength requires --input-image-path"
            )
        return None

    if strength is None:
        strength = DEFAULT_TRANSFORM_STRENGTH
    if strength != strength or strength < 0.0 or strength > 1.0:
        raise ValueError(
            f"image transform strength must be between 0 and 1; got {strength}"
        )
    return strength


def validate_lora_scale(lora_scale: float) -> None:
    if lora_scale != lora_scale or lora_scale < 0.0 or lora_scale > 4.0:
        raise ValueError(f"image LoRA scale must be between 0 and 4; got {lora_scale}")


def write_image_generation_output(
    request: ImageGenerationRequest,
    image: object,
) -> ImageGenerationResult:
    request.output_path.parent.mkdir(parents=True, exist_ok=True)
    if request.output_format == JPG_FORMAT and hasattr(image, "convert"):
        image = image.convert("RGB")
    save_kwargs: dict[str, object] = {}
    if request.output_format == JPG_FORMAT:
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


def _validate_side(axis: str, value: int) -> None:
    if value < 64 or value > 1024:
        raise ValueError(
            f"image generation {axis} must be between 64 and 1024 pixels; got {value}"
        )
    if value % 8 != 0:
        raise ValueError(
            f"image generation {axis} must be divisible by 8 pixels; got {value}"
        )
