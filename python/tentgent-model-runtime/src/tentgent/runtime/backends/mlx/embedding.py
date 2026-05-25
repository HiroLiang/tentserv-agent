from __future__ import annotations

from ..base import MlxBackendModel
from ..embedding import EmbeddingBackendModel, EmbeddingRequest, EmbeddingResult
from ..records import ModelFormat, ModelRecord


_MLX_EMBEDDING_NOT_IMPLEMENTED = (
    "MLX embedding is not implemented in the Apache-licensed runtime. "
    "Use an external or forked backend if you need an MLX embedding package "
    "with a different license boundary."
)


class MlxEmbeddingModel(MlxBackendModel, EmbeddingBackendModel):
    """Dependency-free placeholder for external MLX embedding backends.

    Keep this class free of optional GPL or license-restricted imports. Downstream
    forks can replace the builder target with a concrete implementation.
    """

    def __init__(self) -> None:
        self._record: ModelRecord | None = None

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.MLX:
            raise ValueError(
                f"MLX embedding model cannot load primary_format `{record.primary_format}`"
            )
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None

    def release(self) -> None:
        self._record = None

    def embed(self, request: EmbeddingRequest) -> EmbeddingResult:
        if self._record is None:
            raise RuntimeError("MLX embedding model is not loaded yet; call load() first.")
        raise NotImplementedError(_MLX_EMBEDDING_NOT_IMPLEMENTED)
