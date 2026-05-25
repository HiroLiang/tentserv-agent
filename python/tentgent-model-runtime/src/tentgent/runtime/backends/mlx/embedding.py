from __future__ import annotations

from ..embedding import EmbeddingBackendModel, EmbeddingRequest, EmbeddingResult
from ..records import ModelRecord
from .base import MlxBackendModel, require_mlx_model


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
        require_mlx_model(record, "MLX embedding model")
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
