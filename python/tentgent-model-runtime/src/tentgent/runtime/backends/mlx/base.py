from __future__ import annotations

from abc import ABC

from ..base import BackendConcurrencyPolicy, BackendFamily, BackendModel
from ..records import ModelFormat, ModelRecord


class MlxBackendModel(BackendModel, ABC):
    """Base class for MLX-family backend models."""

    family = BackendFamily.MLX
    concurrency_policy = BackendConcurrencyPolicy.EXCLUSIVE


def require_mlx_model(record: ModelRecord, backend_name: str) -> None:
    if record.primary_format != ModelFormat.MLX:
        raise ValueError(
            f"{backend_name} cannot load primary_format `{record.primary_format}`"
        )


def clear_mlx_cache() -> None:
    try:
        import mlx.core as mx
    except ModuleNotFoundError:
        return

    clear_cache = getattr(mx, "clear_cache", None)
    if callable(clear_cache):
        clear_cache()
        return

    metal = getattr(mx, "metal", None)
    if metal is not None and hasattr(metal, "clear_cache"):
        metal.clear_cache()
