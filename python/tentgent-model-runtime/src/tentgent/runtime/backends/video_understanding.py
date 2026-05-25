from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass, field
from enum import StrEnum
from pathlib import Path
from typing import Any

from .base import BackendModel


DEFAULT_MAX_TOKENS = 128
DEFAULT_SAMPLE_FPS = 1.0
DEFAULT_MAX_FRAMES = 32
DEFAULT_MAX_FRAME_EDGE = 768
MAX_PROMPT_BYTES = 8 * 1024
MAX_CONTEXT_BYTES = 64 * 1024
MLX_VIDEO_SUPPORTED_MODEL_TYPES = (
    "qwen2_vl",
    "qwen2_5_vl",
    "idefics3",
    "llava",
)
_MODEL_TYPE_ALIASES = {
    "qwen2vl": "qwen2_vl",
    "qwen2_vl": "qwen2_vl",
    "qwen2_5vl": "qwen2_5_vl",
    "qwen25vl": "qwen2_5_vl",
    "qwen2_5_vl": "qwen2_5_vl",
    "idefics3": "idefics3",
    "llava": "llava",
    "llavaqwen2": "llava",
    "llava_qwen2": "llava",
}


class VideoUnderstandingModelKind(StrEnum):
    TRANSFORMERS_VIDEO_UNDERSTANDING = "transformers-video-understanding"
    MLX_VLM = "mlx-vlm"


class VideoUnderstandingOutputFormat(StrEnum):
    TEXT = "text"
    JSON = "json"
    MD = "md"


@dataclass(frozen=True, slots=True)
class VideoSamplingOptions:
    sample_fps: float | None = DEFAULT_SAMPLE_FPS
    max_frames: int | None = DEFAULT_MAX_FRAMES
    max_frame_edge: int | None = DEFAULT_MAX_FRAME_EDGE
    clip_start_seconds: float | None = None
    clip_duration_seconds: float | None = None


@dataclass(frozen=True, slots=True)
class VideoFocusRegion:
    x: float
    y: float
    width: float
    height: float
    label: str | None = None


@dataclass(frozen=True, slots=True)
class VideoUnderstandingContext:
    transcript: str | None = None
    notes: tuple[str, ...] = field(default_factory=tuple)


@dataclass(frozen=True, slots=True)
class VideoUnderstandingRequest:
    video_path: Path
    prompt: str
    output_format: VideoUnderstandingOutputFormat = VideoUnderstandingOutputFormat.TEXT
    system_prompt: str | None = None
    max_tokens: int | None = None
    temperature: float | None = None
    sampling: VideoSamplingOptions = field(default_factory=VideoSamplingOptions)
    focus_regions: tuple[VideoFocusRegion, ...] = field(default_factory=tuple)
    context: VideoUnderstandingContext | None = None


@dataclass(frozen=True, slots=True)
class VideoUnderstandingResult:
    output_format: VideoUnderstandingOutputFormat
    media_type: str
    text: str
    finish_reason: str = "stop"
    sampled_frames: int | None = None


class UnsupportedMlxVideoModelError(RuntimeError):
    def __init__(self, model_type: str | None) -> None:
        self.model_type = model_type
        supported = ", ".join(MLX_VIDEO_SUPPORTED_MODEL_TYPES)
        detected = model_type or "unknown"
        super().__init__(
            "MLX VLM video understanding does not support model_type "
            f"`{detected}`; supported model types: {supported}"
        )

    def to_http_detail(self) -> dict[str, object]:
        return {
            "error": "mlx_video_model_unsupported",
            "message": str(self),
            "model_type": self.model_type,
            "supported_model_types": list(MLX_VIDEO_SUPPORTED_MODEL_TYPES),
        }


class VideoUnderstandingBackendModel(BackendModel, ABC):
    @abstractmethod
    def understand_video(
        self,
        request: VideoUnderstandingRequest,
    ) -> VideoUnderstandingResult:
        """Run one video-plus-prompt to text request."""
        raise NotImplementedError


VideoUnderstandingModelFactory = Callable[[Any], VideoUnderstandingBackendModel]


def build_video_understanding_model(kind: Any) -> VideoUnderstandingBackendModel:
    try:
        video_kind = (
            kind
            if isinstance(kind, VideoUnderstandingModelKind)
            else VideoUnderstandingModelKind(kind)
        )
    except ValueError as exc:
        raise ValueError(f"unsupported video understanding model kind `{kind}`") from exc

    if (
        video_kind
        == VideoUnderstandingModelKind.TRANSFORMERS_VIDEO_UNDERSTANDING
    ):
        from .transformers import TransformersVideoUnderstandingModel

        return TransformersVideoUnderstandingModel()
    if video_kind == VideoUnderstandingModelKind.MLX_VLM:
        from .mlx import MlxVlmVideoUnderstandingModel

        return MlxVlmVideoUnderstandingModel()

    raise ValueError(f"unsupported video understanding model kind `{kind}`")


def normalize_video_understanding_request(
    request: VideoUnderstandingRequest,
) -> VideoUnderstandingRequest:
    video_path = request.video_path.expanduser().resolve()
    if not video_path.is_file():
        raise FileNotFoundError(
            f"video understanding video path `{video_path}` was not found"
        )
    if video_path.stat().st_size == 0:
        raise ValueError(
            f"video understanding video path `{video_path}` must not be empty"
        )

    prompt = _normalize_prompt(request.prompt, label="video understanding prompt")
    system_prompt = _normalize_optional_prompt(
        request.system_prompt,
        label="video understanding system prompt",
    )
    output_format = normalize_video_understanding_output_format(request.output_format)
    sampling = normalize_video_sampling_options(request.sampling)
    max_tokens = normalize_video_understanding_max_tokens(request.max_tokens)
    temperature = normalize_video_understanding_temperature(request.temperature)

    return VideoUnderstandingRequest(
        video_path=video_path,
        prompt=prompt,
        system_prompt=system_prompt,
        output_format=output_format,
        max_tokens=max_tokens,
        temperature=temperature,
        sampling=sampling,
        focus_regions=tuple(
            normalize_video_focus_region(region) for region in request.focus_regions
        ),
        context=normalize_video_context(request.context),
    )


def normalize_video_understanding_output_format(
    value: str | VideoUnderstandingOutputFormat,
) -> VideoUnderstandingOutputFormat:
    if isinstance(value, VideoUnderstandingOutputFormat):
        return value
    normalized = value.strip().lower()
    if normalized in {"", "txt"}:
        normalized = VideoUnderstandingOutputFormat.TEXT.value
    if normalized == "markdown":
        normalized = VideoUnderstandingOutputFormat.MD.value
    try:
        return VideoUnderstandingOutputFormat(normalized)
    except ValueError as exc:
        expected = ", ".join(item.value for item in VideoUnderstandingOutputFormat)
        raise ValueError(
            f"unsupported video understanding output format `{value}`; "
            f"expected one of: {expected}"
        ) from exc


def video_understanding_media_type(
    value: str | VideoUnderstandingOutputFormat,
) -> str:
    output_format = normalize_video_understanding_output_format(value)
    if output_format == VideoUnderstandingOutputFormat.JSON:
        return "application/json"
    if output_format == VideoUnderstandingOutputFormat.MD:
        return "text/markdown"
    return "text/plain"


def normalize_video_sampling_options(
    value: VideoSamplingOptions,
) -> VideoSamplingOptions:
    sample_fps = value.sample_fps if value.sample_fps is not None else DEFAULT_SAMPLE_FPS
    max_frames = value.max_frames if value.max_frames is not None else DEFAULT_MAX_FRAMES
    max_frame_edge = (
        value.max_frame_edge
        if value.max_frame_edge is not None
        else DEFAULT_MAX_FRAME_EDGE
    )

    if sample_fps != sample_fps or not 0.1 <= sample_fps <= 4.0:
        raise ValueError(
            f"video understanding sample_fps must be between 0.1 and 4.0; got {sample_fps}"
        )
    if not 1 <= max_frames <= 128:
        raise ValueError(
            f"video understanding max_frames must be between 1 and 128; got {max_frames}"
        )
    if not 128 <= max_frame_edge <= 1536:
        raise ValueError(
            "video understanding max_frame_edge must be between 128 and 1536; "
            f"got {max_frame_edge}"
        )
    if value.clip_start_seconds is not None and value.clip_start_seconds < 0:
        raise ValueError(
            "video understanding clip_start_seconds must be greater than or "
            f"equal to 0; got {value.clip_start_seconds}"
        )
    if value.clip_duration_seconds is not None and value.clip_duration_seconds <= 0:
        raise ValueError(
            "video understanding clip_duration_seconds must be greater than 0; "
            f"got {value.clip_duration_seconds}"
        )

    return VideoSamplingOptions(
        sample_fps=sample_fps,
        max_frames=max_frames,
        max_frame_edge=max_frame_edge,
        clip_start_seconds=value.clip_start_seconds,
        clip_duration_seconds=value.clip_duration_seconds,
    )


def normalize_video_focus_region(region: VideoFocusRegion) -> VideoFocusRegion:
    values = (region.x, region.y, region.width, region.height)
    if any(value != value for value in values):
        raise ValueError("video understanding focus region values must be finite")
    if region.x < 0 or region.y < 0:
        raise ValueError("video understanding focus region x and y must be >= 0")
    if region.width <= 0 or region.height <= 0:
        raise ValueError(
            "video understanding focus region width and height must be greater than 0"
        )
    if region.x + region.width > 1.0 or region.y + region.height > 1.0:
        raise ValueError(
            "video understanding focus region must fit within normalized frame bounds"
        )
    label = region.label.strip() if region.label else None
    return VideoFocusRegion(
        x=region.x,
        y=region.y,
        width=region.width,
        height=region.height,
        label=label or None,
    )


def normalize_video_context(
    context: VideoUnderstandingContext | None,
) -> VideoUnderstandingContext | None:
    if context is None:
        return None
    transcript = _normalize_optional_context(
        context.transcript,
        label="video understanding transcript",
    )
    notes = tuple(
        note
        for note in (
            _normalize_optional_context(
                note,
                label="video understanding context note",
            )
            for note in context.notes
        )
        if note is not None
    )
    if transcript is None and not notes:
        return None
    return VideoUnderstandingContext(transcript=transcript, notes=notes)


def normalize_video_understanding_max_tokens(value: int | None) -> int | None:
    if value is None:
        return None
    if value < 1 or value > 4096:
        raise ValueError(
            f"video understanding max_tokens must be between 1 and 4096; got {value}"
        )
    return value


def normalize_video_understanding_temperature(value: float | None) -> float | None:
    if value is None:
        return None
    if value != value or value < 0.0 or value > 2.0:
        raise ValueError(
            f"video understanding temperature must be between 0 and 2; got {value}"
        )
    return value


def render_video_prompt_text(request: VideoUnderstandingRequest) -> str:
    lines: list[str] = []
    if request.focus_regions:
        lines.append("Focus regions use normalized frame coordinates:")
        for index, region in enumerate(request.focus_regions, start=1):
            label = f" `{region.label}`" if region.label else ""
            lines.append(
                f"- region {index}{label}: x={region.x:.4f}, y={region.y:.4f}, "
                f"width={region.width:.4f}, height={region.height:.4f}"
            )
    if request.context is not None:
        if request.context.transcript:
            lines.append("Transcript:")
            lines.append(request.context.transcript)
        if request.context.notes:
            lines.append("Context notes:")
            for note in request.context.notes:
                lines.append(f"- {note}")
    if lines:
        lines.append("User request:")
    lines.append(request.prompt)
    return "\n".join(lines)


def detect_model_type(config: Any) -> str | None:
    for attr in ("model_type", "model_type_", "architectures"):
        value = _config_value(config, attr)
        if isinstance(value, str) and value.strip():
            return value.strip().lower()
        if isinstance(value, (list, tuple)) and value:
            first = value[0]
            if isinstance(first, str) and first.strip():
                return _normalize_architecture_name(first)
    text_config = _config_value(config, "text_config")
    if text_config is not None:
        nested = detect_model_type(text_config)
        if nested:
            return nested
    vision_config = _config_value(config, "vision_config")
    if vision_config is not None:
        nested = detect_model_type(vision_config)
        if nested:
            return nested
    return None


def ensure_mlx_video_model_supported(config: Any) -> str:
    model_type = detect_model_type(config)
    if model_type is not None:
        model_type = _MODEL_TYPE_ALIASES.get(model_type, model_type)
    if model_type not in MLX_VIDEO_SUPPORTED_MODEL_TYPES:
        raise UnsupportedMlxVideoModelError(model_type)
    return model_type


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


def _normalize_optional_context(value: str | None, *, label: str) -> str | None:
    if value is None:
        return None
    text = value.strip()
    if not text:
        return None
    if len(text.encode("utf-8")) > MAX_CONTEXT_BYTES:
        raise ValueError(f"{label} must be at most {MAX_CONTEXT_BYTES} bytes")
    return text


def _config_value(config: Any, key: str) -> Any:
    if isinstance(config, dict):
        return config.get(key)
    return getattr(config, key, None)


def _normalize_architecture_name(value: str) -> str:
    normalized = value.strip().lower()
    for suffix in ("forconditionalgeneration", "model"):
        normalized = normalized.removesuffix(suffix)
    normalized = normalized.replace("-", "_")
    normalized = normalized.replace("qwen2vl", "qwen2_vl")
    normalized = normalized.replace("qwen2_5vl", "qwen2_5_vl")
    return normalized
