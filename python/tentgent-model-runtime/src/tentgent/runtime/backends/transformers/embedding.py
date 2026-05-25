from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..embedding import (
    EmbeddingBackendModel,
    EmbeddingRequest,
    EmbeddingResult,
    EmbeddingVector,
)
from ..errors import missing_backend_dependency
from ..records import ModelRecord
from .base import (
    TransformersBackendModel,
    clear_torch_device_cache,
    detect_torch_device,
    load_transformers_component,
    load_transformers_model,
    move_batch_to_device,
    require_safetensors_model,
)


@dataclass(frozen=True, slots=True)
class _TransformersEmbeddingDeps:
    torch: Any
    AutoModel: Any
    AutoTokenizer: Any


class TransformersEmbeddingModel(TransformersBackendModel, EmbeddingBackendModel):
    def __init__(self) -> None:
        self._deps = _load_transformers_embedding_deps()
        self._record: ModelRecord | None = None
        self._tokenizer: Any | None = None
        self._model: Any | None = None
        self._device = detect_torch_device(self._deps.torch)

    def load(self, record: ModelRecord) -> None:
        require_safetensors_model(record, "Transformers embedding model")

        load_path = str(record.source_path)
        tokenizer = load_transformers_component(self._deps.AutoTokenizer, load_path)
        model = load_transformers_model(
            self._deps.AutoModel,
            load_path,
            self._device,
        )

        self._record = record
        self._tokenizer = tokenizer
        self._model = model

    @property
    def is_loaded(self) -> bool:
        return (
            self._record is not None
            and self._tokenizer is not None
            and self._model is not None
        )

    def release(self) -> None:
        self._record = None
        self._tokenizer = None
        self._model = None
        clear_torch_device_cache(self._deps.torch)

    def embed(self, request: EmbeddingRequest) -> EmbeddingResult:
        tokenizer, model = self._require_loaded()
        encoded = tokenizer(
            list(request.inputs),
            padding=True,
            truncation=True,
            return_tensors="pt",
        )
        encoded = move_batch_to_device(encoded, self._device)

        with self._deps.torch.inference_mode():
            outputs = model(**encoded)

        token_embeddings = outputs.last_hidden_state
        attention_mask = encoded["attention_mask"].unsqueeze(-1).expand(
            token_embeddings.size()
        )
        attention_mask = attention_mask.float()
        sum_embeddings = (token_embeddings * attention_mask).sum(dim=1)
        sum_mask = attention_mask.sum(dim=1).clamp(min=1e-9)
        vectors = sum_embeddings / sum_mask
        vectors = self._deps.torch.nn.functional.normalize(vectors, p=2, dim=1)
        data = tuple(
            EmbeddingVector(index=index, embedding=vector)
            for index, vector in enumerate(vectors.detach().cpu().tolist())
        )
        return EmbeddingResult(data=data)

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers embedding model is not loaded yet; call load() first."
            )
        return self._tokenizer, self._model


def _load_transformers_embedding_deps() -> _TransformersEmbeddingDeps:
    try:
        import torch
        from transformers import AutoModel, AutoTokenizer
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "transformers"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _TransformersEmbeddingDeps(
        torch=torch,
        AutoModel=AutoModel,
        AutoTokenizer=AutoTokenizer,
    )
