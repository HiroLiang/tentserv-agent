from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_vision_chat_backend


TEXT_FORMAT = "text"
JSON_FORMAT = "json"
MD_FORMAT = "md"
SUPPORTED_OUTPUT_FORMATS = {TEXT_FORMAT, JSON_FORMAT, MD_FORMAT}


@dataclass(frozen=True)
class VisionChatRequest:
    model_ref: str
    image_path: Path
    prompt: str
    output_format: str
    system_prompt: str | None = None
    max_tokens: int | None = None
    temperature: float | None = None


@dataclass(frozen=True)
class VisionChatResult:
    output_format: str
    media_type: str
    text: str
    finish_reason: str = "stop"


@dataclass(frozen=True)
class VisionChatPlan:
    request: VisionChatRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_vision_chat_plan(
    request: VisionChatRequest,
    home: Path | None = None,
) -> VisionChatPlan:
    record = load_model_record(request.model_ref, home=home)
    if "vision-chat" not in record.model_capabilities:
        capabilities = ", ".join(record.model_capabilities)
        raise ValueError(
            "vision chat endpoint requires model capability "
            f"`vision-chat`, but model `{record.model_ref}` advertises "
            f"[{capabilities}]"
        )

    image_path = request.image_path.expanduser().resolve()
    if not image_path.is_file():
        raise FileNotFoundError(f"vision chat image path `{image_path}` was not found")

    prompt = request.prompt.strip()
    if not prompt:
        raise ValueError("vision chat prompt must not be empty")

    system_prompt = request.system_prompt.strip() if request.system_prompt else None
    if system_prompt == "":
        system_prompt = None

    output_format = normalize_vision_chat_output_format(request.output_format)
    return VisionChatPlan(
        request=VisionChatRequest(
            model_ref=request.model_ref,
            image_path=image_path,
            prompt=prompt,
            system_prompt=system_prompt,
            output_format=output_format,
            max_tokens=request.max_tokens,
            temperature=request.temperature,
        ),
        record=record,
        backend=resolve_vision_chat_backend(record),
        load_path=record.variant_source_path,
    )


def normalize_vision_chat_output_format(value: str) -> str:
    normalized = value.strip().lower()
    if normalized in {"txt", ""}:
        normalized = TEXT_FORMAT
    if normalized == "markdown":
        normalized = MD_FORMAT
    if normalized not in SUPPORTED_OUTPUT_FORMATS:
        expected = ", ".join(sorted(SUPPORTED_OUTPUT_FORMATS))
        raise ValueError(
            f"unsupported vision chat output format `{value}`; "
            f"expected one of: {expected}"
        )
    return normalized


def vision_chat_media_type(output_format: str) -> str:
    output_format = normalize_vision_chat_output_format(output_format)
    if output_format == JSON_FORMAT:
        return "application/json"
    if output_format == MD_FORMAT:
        return "text/markdown"
    return "text/plain"
