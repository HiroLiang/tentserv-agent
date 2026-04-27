from __future__ import annotations

from enum import StrEnum

from .capabilities import ensure_backend_supported
from .records import StoredModelRecord


class BackendKind(StrEnum):
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
