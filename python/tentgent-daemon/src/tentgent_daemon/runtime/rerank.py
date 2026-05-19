from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_rerank_backend


@dataclass(frozen=True)
class RerankRequest:
    model_ref: str
    query: str
    documents: tuple[str, ...]
    top_n: int | None = None


@dataclass(frozen=True)
class RerankScore:
    index: int
    score: float


@dataclass(frozen=True)
class RerankResult:
    data: tuple[RerankScore, ...]


@dataclass(frozen=True)
class RerankPlan:
    request: RerankRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_rerank_plan(
    request: RerankRequest,
    home: Path | None = None,
) -> RerankPlan:
    record = load_model_record(request.model_ref, home=home)
    if "rerank" not in record.model_capabilities:
        capabilities = ", ".join(record.model_capabilities)
        raise ValueError(
            f"rerank endpoint requires model capability `rerank`, "
            f"but model `{record.model_ref}` advertises [{capabilities}]"
        )

    return RerankPlan(
        request=request,
        record=record,
        backend=resolve_rerank_backend(record),
        load_path=record.variant_source_path,
    )


def ranked_scores(scores: list[float], top_n: int | None = None) -> RerankResult:
    ranked = [
        RerankScore(index=index, score=float(score))
        for index, score in enumerate(scores)
    ]
    ranked.sort(key=lambda item: (-item.score, item.index))
    if top_n is not None:
        ranked = ranked[:top_n]
    return RerankResult(data=tuple(ranked))
