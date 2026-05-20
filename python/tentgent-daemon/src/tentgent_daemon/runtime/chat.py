from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_backend


@dataclass(frozen=True)
class Message:
    role: str
    content: str


@dataclass(frozen=True)
class ChatRequest:
    model_ref: str
    messages: tuple[Message, ...]
    max_tokens: int | None = None
    temperature: float | None = None
    adapter_ref: str | None = None


@dataclass(frozen=True)
class ChatPlan:
    request: ChatRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_chat_plan(request: ChatRequest, home: Path | None = None) -> ChatPlan:
    record = load_model_record(request.model_ref, home=home)
    return ChatPlan(
        request=request,
        record=record,
        backend=resolve_backend(record),
        load_path=record.variant_source_path,
    )
