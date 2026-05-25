from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..errors import missing_backend_dependency
from ..records import ModelRecord
from ..rerank import RerankBackendModel, RerankRequest, RerankResult, ranked_scores
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
class _TransformersRerankDeps:
    torch: Any
    AutoModelForSequenceClassification: Any
    AutoTokenizer: Any


class TransformersRerankModel(TransformersBackendModel, RerankBackendModel):
    def __init__(self) -> None:
        self._deps = _load_transformers_rerank_deps()
        self._record: ModelRecord | None = None
        self._tokenizer: Any | None = None
        self._model: Any | None = None
        self._device = detect_torch_device(self._deps.torch)

    def load(self, record: ModelRecord) -> None:
        require_safetensors_model(record, "Transformers rerank model")

        load_path = str(record.source_path)
        tokenizer = load_transformers_component(self._deps.AutoTokenizer, load_path)
        model = load_transformers_model(
            self._deps.AutoModelForSequenceClassification,
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

    def rerank(self, request: RerankRequest) -> RerankResult:
        tokenizer, model = self._require_loaded()
        encoded = tokenizer(
            [request.query] * len(request.documents),
            list(request.documents),
            padding=True,
            truncation=True,
            return_tensors="pt",
        )
        encoded = move_batch_to_device(encoded, self._device)

        with self._deps.torch.inference_mode():
            outputs = model(**encoded)

        logits = outputs.logits
        if len(logits.shape) == 1:
            scores_tensor = logits
        elif logits.shape[-1] == 1:
            scores_tensor = logits.squeeze(-1)
        else:
            scores_tensor = logits[:, -1]
        scores = scores_tensor.detach().float().cpu().tolist()
        return ranked_scores(scores, request.top_n)

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers rerank model is not loaded yet; call load() first."
            )
        return self._tokenizer, self._model


def _load_transformers_rerank_deps() -> _TransformersRerankDeps:
    try:
        import torch
        from transformers import AutoModelForSequenceClassification, AutoTokenizer
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "transformers"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _TransformersRerankDeps(
        torch=torch,
        AutoModelForSequenceClassification=AutoModelForSequenceClassification,
        AutoTokenizer=AutoTokenizer,
    )
