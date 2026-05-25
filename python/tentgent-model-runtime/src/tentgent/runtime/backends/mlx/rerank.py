from __future__ import annotations

from ..base import MlxBackendModel
from ..records import ModelFormat, ModelRecord
from ..rerank import RerankBackendModel, RerankRequest, RerankResult


_MLX_RERANK_NOT_IMPLEMENTED = (
    "MLX rerank is not implemented in the Apache-licensed runtime. "
    "Use an external or forked backend if you need an MLX reranker package "
    "with a different license boundary."
)


class MlxRerankModel(MlxBackendModel, RerankBackendModel):
    """Dependency-free placeholder for external MLX rerank backends.

    Keep this class free of optional GPL or license-restricted imports. Downstream
    forks can replace the builder target with a concrete implementation.
    """

    def __init__(self) -> None:
        self._record: ModelRecord | None = None

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.MLX:
            raise ValueError(
                f"MLX rerank model cannot load primary_format `{record.primary_format}`"
            )
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None

    def release(self) -> None:
        self._record = None

    def rerank(self, request: RerankRequest) -> RerankResult:
        if self._record is None:
            raise RuntimeError("MLX rerank model is not loaded yet; call load() first.")
        raise NotImplementedError(_MLX_RERANK_NOT_IMPLEMENTED)
