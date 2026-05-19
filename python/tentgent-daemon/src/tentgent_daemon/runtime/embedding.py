from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_embedding_backend


@dataclass(frozen=True)
class EmbeddingRequest:
    model_ref: str
    inputs: tuple[str, ...]


@dataclass(frozen=True)
class EmbeddingPlan:
    request: EmbeddingRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_embedding_plan(
    request: EmbeddingRequest,
    home: Path | None = None,
) -> EmbeddingPlan:
    record = load_model_record(request.model_ref, home=home)
    if "embedding" not in record.model_capabilities:
        capabilities = ", ".join(record.model_capabilities)
        raise ValueError(
            f"embedding endpoint requires model capability `embedding`, "
            f"but model `{record.model_ref}` advertises [{capabilities}]"
        )

    return EmbeddingPlan(
        request=request,
        record=record,
        backend=resolve_embedding_backend(record),
        load_path=record.variant_source_path,
    )
