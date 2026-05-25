from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import Any

from .base import BackendModel


DEFAULT_MAX_TOKENS = 128
MAX_PROMPT_BYTES = 8 * 1024


class VisionChatModelKind(StrEnum):
    TRANSFORMERS_IMAGE_TEXT_TO_TEXT = "transformers-image-text-to-text"
    MLX_VLM = "mlx-vlm"


class VisionChatOutputFormat(StrEnum):
    TEXT = "text"
    JSON = "json"
    MD = "md"


@dataclass(frozen=True, slots=True)
class VisionChatRequest:
    image_path: Path
    prompt: str
    output_format: VisionChatOutputFormat = VisionChatOutputFormat.TEXT
    image_media_type: str | None = None
    system_prompt: str | None = None
    max_tokens: int | None = None
    temperature: float | None = None


@dataclass(frozen=True, slots=True)
class VisionChatResult:
    output_format: VisionChatOutputFormat
    media_type: str
    text: str
    finish_reason: str = "stop"


class VisionChatBackendModel(BackendModel, ABC):
    @abstractmethod
    def generate_vision_chat(self, request: VisionChatRequest) -> VisionChatResult:
        """Run one image-plus-prompt to text request."""
        raise NotImplementedError


VisionChatModelFactory = Callable[[Any], VisionChatBackendModel]


def build_vision_chat_model(kind: Any) -> VisionChatBackendModel:
    try:
        vision_kind = (
            kind if isinstance(kind, VisionChatModelKind) else VisionChatModelKind(kind)
        )
    except ValueError as exc:
        raise ValueError(f"unsupported vision chat model kind `{kind}`") from exc

    if vision_kind == VisionChatModelKind.TRANSFORMERS_IMAGE_TEXT_TO_TEXT:
        from .transformers import TransformersVisionChatModel

        return TransformersVisionChatModel()
    if vision_kind == VisionChatModelKind.MLX_VLM:
        from .mlx import MlxVlmVisionChatModel

        return MlxVlmVisionChatModel()

    raise ValueError(f"unsupported vision chat model kind `{kind}`")


def normalize_vision_chat_request(request: VisionChatRequest) -> VisionChatRequest:
    image_path = request.image_path.expanduser().resolve()
    if not image_path.is_file():
        raise FileNotFoundError(f"vision chat image path `{image_path}` was not found")
    if image_path.stat().st_size == 0:
        raise ValueError(f"vision chat image path `{image_path}` must not be empty")

    prompt = _normalize_prompt(request.prompt, label="vision chat prompt")
    system_prompt = _normalize_optional_prompt(
        request.system_prompt,
        label="vision chat system prompt",
    )
    output_format = normalize_vision_chat_output_format(request.output_format)
    max_tokens = normalize_vision_chat_max_tokens(request.max_tokens)
    temperature = normalize_vision_chat_temperature(request.temperature)

    return VisionChatRequest(
        image_path=image_path,
        image_media_type=normalize_vision_chat_image_media_type(
            request.image_media_type
        ),
        prompt=prompt,
        system_prompt=system_prompt,
        output_format=output_format,
        max_tokens=max_tokens,
        temperature=temperature,
    )


def normalize_vision_chat_output_format(
    value: str | VisionChatOutputFormat,
) -> VisionChatOutputFormat:
    if isinstance(value, VisionChatOutputFormat):
        return value
    normalized = value.strip().lower()
    if normalized in {"", "txt"}:
        normalized = VisionChatOutputFormat.TEXT.value
    if normalized == "markdown":
        normalized = VisionChatOutputFormat.MD.value
    try:
        return VisionChatOutputFormat(normalized)
    except ValueError as exc:
        expected = ", ".join(item.value for item in VisionChatOutputFormat)
        raise ValueError(
            f"unsupported vision chat output format `{value}`; "
            f"expected one of: {expected}"
        ) from exc


def vision_chat_media_type(value: str | VisionChatOutputFormat) -> str:
    output_format = normalize_vision_chat_output_format(value)
    if output_format == VisionChatOutputFormat.JSON:
        return "application/json"
    if output_format == VisionChatOutputFormat.MD:
        return "text/markdown"
    return "text/plain"


def normalize_vision_chat_image_media_type(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.strip().lower().split(";")[0]
    return normalized or None


def normalize_vision_chat_max_tokens(value: int | None) -> int | None:
    if value is None:
        return None
    if value < 1 or value > 4096:
        raise ValueError(f"vision chat max_tokens must be between 1 and 4096; got {value}")
    return value


def normalize_vision_chat_temperature(value: float | None) -> float | None:
    if value is None:
        return None
    if value != value or value < 0.0 or value > 2.0:
        raise ValueError(
            f"vision chat temperature must be between 0 and 2; got {value}"
        )
    return value


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
