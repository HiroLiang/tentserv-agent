from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path


class ModelFormat(StrEnum):
    SAFETENSORS = "safetensors"
    GGUF = "gguf"
    MLX = "mlx"
    DIFFUSERS = "diffusers"


class ModelCapability(StrEnum):
    CHAT = "chat"
    EMBEDDING = "embedding"
    RERANK = "rerank"
    AUDIO_TRANSCRIPTION = "audio-transcription"
    AUDIO_SPEECH = "audio-speech"
    VISION_CHAT = "vision-chat"
    VIDEO_UNDERSTANDING = "video-understanding"
    IMAGE_GENERATION = "image-generation"


class AdapterType(StrEnum):
    LORA = "lora"
    CONTROLNET = "controlnet"


@dataclass(frozen=True, slots=True)
class ModelRecord:
    """Runtime-facing description of a managed model resource.

    Rust owns task routing and backend selection. This record only describes the
    resource that a concrete backend implementation was asked to load.
    """

    model_ref: str
    source_path: Path
    primary_format: ModelFormat
    capabilities: frozenset[ModelCapability] = frozenset()
    short_ref: str | None = None
    source_repo: str | None = None
    source_revision: str | None = None


@dataclass(frozen=True, slots=True)
class AdapterRecord:
    """Runtime-facing description of an adapter selected for execution.

    Rust owns compatibility checks and task routing. Python backends only need
    the adapter identity and local files required by the underlying runtime.
    """

    adapter_ref: str
    source_path: Path
    adapter_format: str
    adapter_type: AdapterType
    short_ref: str | None = None
    weight_file: str | None = None
