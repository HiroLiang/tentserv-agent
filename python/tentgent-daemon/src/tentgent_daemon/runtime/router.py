from __future__ import annotations

from enum import StrEnum

from .capabilities import ensure_backend_supported
from .records import StoredModelRecord


class BackendKind(StrEnum):
    DIFFUSERS = "diffusers"
    MLX = "mlx"
    MLX_AUDIO = "mlx_audio"
    MLX_DIFFUSION = "mlx_diffusion"
    MLX_VLM = "mlx_vlm"
    TRANSFORMERS_PEFT = "transformers_peft"
    LLAMA_CPP = "llama_cpp"


def resolve_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "mlx":
        if record.mlx_runtime_family not in (None, "mlx-lm"):
            raise ValueError(
                f"unsupported MLX runtime family `{record.mlx_runtime_family}` "
                f"for chat model `{record.model_ref}`; expected `mlx-lm`"
            )
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
    if record.primary_format == "mlx" and record.mlx_runtime_family == "mlx-audio":
        _raise_planned_mlx_backend(record, "audio transcription", "mlx-audio")

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for audio transcription model `{record.model_ref}`"
    )


def resolve_vision_chat_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "safetensors":
        return BackendKind.TRANSFORMERS_PEFT
    if record.primary_format == "mlx" and record.mlx_runtime_family == "mlx-vlm":
        _raise_planned_mlx_backend(record, "vision chat", "mlx-vlm")

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for vision chat model `{record.model_ref}`"
    )


def resolve_image_generation_backend(record: StoredModelRecord) -> BackendKind:
    if record.primary_format == "diffusers":
        return BackendKind.DIFFUSERS
    if record.primary_format == "mlx" and record.mlx_runtime_family == "mlx-diffusion":
        _raise_planned_mlx_backend(record, "image generation", "mlx-diffusion")

    raise ValueError(
        f"unsupported primary_format `{record.primary_format}` for image generation model `{record.model_ref}`"
    )


def _raise_planned_mlx_backend(
    record: StoredModelRecord,
    capability_label: str,
    family: str,
) -> None:
    raise ValueError(
        f"MLX runtime family `{family}` is recorded for {capability_label} model "
        f"`{record.model_ref}`, but that backend is planned and not implemented yet"
    )
