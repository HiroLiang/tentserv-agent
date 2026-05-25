from __future__ import annotations

from abc import ABC

from ..base import BackendConcurrencyPolicy, BackendFamily, BackendModel
from ..records import ModelFormat, ModelRecord


class LlamaCppBackendModel(BackendModel, ABC):
    """Base class for llama.cpp / GGUF backend models."""

    family = BackendFamily.LLAMA_CPP
    concurrency_policy = BackendConcurrencyPolicy.EXCLUSIVE


def require_gguf_model(record: ModelRecord, backend_name: str) -> None:
    if record.primary_format != ModelFormat.GGUF:
        raise ValueError(
            f"{backend_name} cannot load primary_format `{record.primary_format}`"
        )
