from __future__ import annotations

from pathlib import Path

from pydantic import BaseModel, Field

from tentgent.runtime.backends.records import (
    AdapterRecord,
    AdapterType,
    ModelCapability,
    ModelFormat,
    ModelRecord,
)


class ModelRecordPayload(BaseModel):
    model_ref: str
    source_path: str
    primary_format: ModelFormat
    capabilities: list[ModelCapability] = Field(default_factory=list)
    short_ref: str | None = None
    source_repo: str | None = None
    source_revision: str | None = None


class AdapterRecordPayload(BaseModel):
    adapter_ref: str
    source_path: str
    adapter_format: str
    adapter_type: AdapterType = AdapterType.LORA
    short_ref: str | None = None
    weight_file: str | None = None


def model_record(payload: ModelRecordPayload) -> ModelRecord:
    return ModelRecord(
        model_ref=payload.model_ref,
        source_path=Path(payload.source_path),
        primary_format=payload.primary_format,
        capabilities=frozenset(payload.capabilities),
        short_ref=payload.short_ref,
        source_repo=payload.source_repo,
        source_revision=payload.source_revision,
    )


def adapter_record(payload: AdapterRecordPayload | None) -> AdapterRecord | None:
    if payload is None:
        return None
    return AdapterRecord(
        adapter_ref=payload.adapter_ref,
        source_path=Path(payload.source_path),
        adapter_format=payload.adapter_format,
        adapter_type=payload.adapter_type,
        short_ref=payload.short_ref,
        weight_file=payload.weight_file,
    )
