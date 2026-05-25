from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from typing import Any

from .base import BackendModel


class EmbeddingModelKind(StrEnum):
    TRANSFORMERS = "transformers-embedding"
    MLX = "mlx-embedding"
    LLAMA_CPP = "llama-cpp-embedding"


@dataclass(frozen=True, slots=True)
class EmbeddingRequest:
    inputs: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class EmbeddingVector:
    index: int
    embedding: list[float]


@dataclass(frozen=True, slots=True)
class EmbeddingResult:
    data: tuple[EmbeddingVector, ...]


class EmbeddingBackendModel(BackendModel, ABC):
    @abstractmethod
    def embed(self, request: EmbeddingRequest) -> EmbeddingResult:
        """Run an embedding inference request."""
        raise NotImplementedError


EmbeddingModelFactory = Callable[[Any], EmbeddingBackendModel]


def build_embedding_model(kind: Any) -> EmbeddingBackendModel:
    try:
        embedding_kind = (
            kind if isinstance(kind, EmbeddingModelKind) else EmbeddingModelKind(kind)
        )
    except ValueError as exc:
        raise ValueError(f"unsupported embedding model kind `{kind}`") from exc

    if embedding_kind == EmbeddingModelKind.TRANSFORMERS:
        from .transformers import TransformersEmbeddingModel

        return TransformersEmbeddingModel()
    if embedding_kind == EmbeddingModelKind.MLX:
        from .mlx import MlxEmbeddingModel

        return MlxEmbeddingModel()
    if embedding_kind == EmbeddingModelKind.LLAMA_CPP:
        from .llama_cpp import LlamaCppEmbeddingModel

        return LlamaCppEmbeddingModel()

    raise ValueError(f"unsupported embedding model kind `{kind}`")
