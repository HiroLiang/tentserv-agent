from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..base import TransformersBackendModel
from ..embedding import (
    EmbeddingBackendModel,
    EmbeddingRequest,
    EmbeddingResult,
    EmbeddingVector,
)
from ..errors import missing_backend_dependency
from ..records import ModelFormat, ModelRecord


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
        self._device = _detect_device(self._deps.torch)

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.SAFETENSORS:
            raise ValueError(
                "Transformers embedding model cannot load "
                f"primary_format `{record.primary_format}`"
            )

        load_path = str(record.source_path)
        tokenizer = self._deps.AutoTokenizer.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model = self._deps.AutoModel.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model.to(self._device)
        model.eval()

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
        _clear_device_cache(self._deps.torch)

    def embed(self, request: EmbeddingRequest) -> EmbeddingResult:
        tokenizer, model = self._require_loaded()
        encoded = tokenizer(
            list(request.inputs),
            padding=True,
            truncation=True,
            return_tensors="pt",
        )
        encoded = {key: value.to(self._device) for key, value in encoded.items()}

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


def _detect_device(torch: Any) -> Any:
    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _clear_device_cache(torch: Any) -> None:
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    if torch.backends.mps.is_available():
        torch.mps.empty_cache()
