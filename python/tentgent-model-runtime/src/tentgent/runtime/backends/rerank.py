from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from typing import Any

from .base import BackendModel


class RerankModelKind(StrEnum):
    TRANSFORMERS = "transformers-rerank"
    MLX = "mlx-rerank"


@dataclass(frozen=True, slots=True)
class RerankRequest:
    query: str
    documents: tuple[str, ...]
    top_n: int | None = None


@dataclass(frozen=True, slots=True)
class RerankScore:
    index: int
    score: float


@dataclass(frozen=True, slots=True)
class RerankResult:
    data: tuple[RerankScore, ...]


class RerankBackendModel(BackendModel, ABC):
    @abstractmethod
    def rerank(self, request: RerankRequest) -> RerankResult:
        """Run a rerank inference request."""
        raise NotImplementedError


RerankModelFactory = Callable[[Any], RerankBackendModel]


def build_rerank_model(kind: Any) -> RerankBackendModel:
    try:
        rerank_kind = kind if isinstance(kind, RerankModelKind) else RerankModelKind(kind)
    except ValueError as exc:
        raise ValueError(f"unsupported rerank model kind `{kind}`") from exc

    if rerank_kind == RerankModelKind.TRANSFORMERS:
        from .transformers import TransformersRerankModel

        return TransformersRerankModel()
    if rerank_kind == RerankModelKind.MLX:
        from .mlx import MlxRerankModel

        return MlxRerankModel()

    raise ValueError(f"unsupported rerank model kind `{kind}`")


def ranked_scores(scores: list[float], top_n: int | None = None) -> RerankResult:
    ranked = [
        RerankScore(index=index, score=float(score))
        for index, score in enumerate(scores)
    ]
    ranked.sort(key=lambda item: (-item.score, item.index))
    if top_n is not None:
        ranked = ranked[:top_n]
    return RerankResult(data=tuple(ranked))
