from __future__ import annotations

from enum import StrEnum

from .capabilities import ensure_backend_supported
from .records import StoredModelRecord


class BackendKind(StrEnum):
    DIFFUSERS = "diffusers"
    MLX = "mlx"
    TRANSFORMERS_PEFT = "transformers_peft"
    LLAMA_CPP = "llama_cpp"


def resolve_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "mlx":
        backend = BackendKind.MLX
        ensure_backend_supported(str(backend))
        return backend
    if record.primary_format == "safetensors":
        return BackendKind.TRANSFORMERS_PEFT
    if record.primary_format == "gguf":
        return BackendKind.LLAMA_CPP

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for model `{record.model_ref}`"
    )


def resolve_embedding_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "safetensors":
        return BackendKind.TRANSFORMERS_PEFT

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for embedding model `{record.model_ref}`"
    )


def resolve_rerank_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "safetensors":
        return BackendKind.TRANSFORMERS_PEFT

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for rerank model `{record.model_ref}`"
    )


def resolve_audio_transcription_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "safetensors":
        return BackendKind.TRANSFORMERS_PEFT

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for audio transcription model `{record.model_ref}`"
    )


def resolve_vision_chat_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "safetensors":
        return BackendKind.TRANSFORMERS_PEFT

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for vision chat model `{record.model_ref}`"
    )


def resolve_image_generation_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "diffusers":
        return BackendKind.DIFFUSERS

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for image generation model `{record.model_ref}`"
    )
