from __future__ import annotations

from typing import Any

from ..embedding import (
    EmbeddingBackendModel,
    EmbeddingRequest,
    EmbeddingResult,
    EmbeddingVector,
)
from ..records import ModelRecord
from .base import LlamaCppBackendModel, require_gguf_model
from .common import load_llama_class, resolve_gguf_path


class LlamaCppEmbeddingModel(LlamaCppBackendModel, EmbeddingBackendModel):
    def __init__(self) -> None:
        self._record: ModelRecord | None = None
        self._model: Any | None = None

    def load(self, record: ModelRecord) -> None:
        require_gguf_model(record, "llama.cpp embedding model")

        llama = load_llama_class()
        model_path = resolve_gguf_path(record.source_path)
        self._model = llama(
            model_path=str(model_path),
            embedding=True,
            verbose=False,
        )
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None and self._model is not None

    def release(self) -> None:
        self._record = None
        self._model = None

    def embed(self, request: EmbeddingRequest) -> EmbeddingResult:
        model = self._require_loaded()
        response = model.create_embedding(input=list(request.inputs))
        data = tuple(
            EmbeddingVector(
                index=int(item.get("index", index)),
                embedding=[float(value) for value in item["embedding"]],
            )
            for index, item in enumerate(response.get("data", []))
        )
        if len(data) != len(request.inputs):
            raise RuntimeError("llama.cpp embedding response length did not match input")
        return EmbeddingResult(data=data)

    def _require_loaded(self) -> Any:
        if self._record is None or self._model is None:
            raise RuntimeError(
                "llama.cpp embedding model is not loaded yet; call load() first."
            )
        return self._model
