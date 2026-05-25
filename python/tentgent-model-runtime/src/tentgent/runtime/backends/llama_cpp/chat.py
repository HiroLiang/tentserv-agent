from __future__ import annotations

from collections.abc import Iterator
from pathlib import Path
from typing import Any

from ..base import LlamaCppBackendModel
from ..chat import ChatBackendModel, ChatMessage, ChatRequest, ChatResult
from ..errors import missing_backend_dependency
from ..records import ModelFormat, ModelRecord


class LlamaCppChatModel(LlamaCppBackendModel, ChatBackendModel):
    def __init__(self) -> None:
        self._record: ModelRecord | None = None
        self._model: Any | None = None

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.GGUF:
            raise ValueError(
                "llama.cpp chat model cannot load "
                f"primary_format `{record.primary_format}`"
            )

        llama = _load_llama_class()
        model_path = _resolve_gguf_path(record.source_path)
        self._model = llama(
            model_path=str(model_path),
            n_ctx=2048,
            verbose=False,
        )
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None and self._model is not None

    def release(self) -> None:
        self._record = None
        self._model = None

    def generate(self, request: ChatRequest) -> ChatResult:
        model = self._require_loaded()
        response = model.create_chat_completion(
            messages=_render_messages(request.messages),
            temperature=0.0 if request.temperature is None else request.temperature,
            max_tokens=request.max_tokens or 128,
            stream=False,
        )
        text = response["choices"][0]["message"]["content"]
        return ChatResult(text=(text or "").rstrip())

    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        model = self._require_loaded()
        response = model.create_chat_completion(
            messages=_render_messages(request.messages),
            temperature=0.0 if request.temperature is None else request.temperature,
            max_tokens=request.max_tokens or 128,
            stream=True,
        )
        for chunk in response:
            choices = chunk.get("choices", [])
            if not choices:
                continue
            delta = choices[0].get("delta", {})
            content = delta.get("content")
            if content:
                yield content

    def _require_loaded(self) -> Any:
        if self._record is None or self._model is None:
            raise RuntimeError(
                "llama.cpp chat model is not loaded yet; call load() first."
            )
        return self._model


def _load_llama_class() -> Any:
    try:
        from llama_cpp import Llama
    except ModuleNotFoundError as exc:
        if exc.name == "llama_cpp":
            raise missing_backend_dependency(exc.name) from exc
        raise
    return Llama


def _resolve_gguf_path(source_path: Path) -> Path:
    if source_path.is_file():
        if source_path.suffix.lower() == ".gguf":
            return source_path
        raise ValueError(f"expected a GGUF file, got `{source_path}`")

    matches = sorted(source_path.glob("*.gguf"))
    if not matches:
        raise FileNotFoundError(f"no GGUF file found under `{source_path}`")
    if len(matches) > 1:
        names = ", ".join(path.name for path in matches[:5])
        raise ValueError(
            "multiple GGUF files were found in the model source; "
            f"this runtime expects exactly one GGUF file (found: {names})"
        )
    return matches[0]


def _render_messages(messages: tuple[ChatMessage, ...]) -> list[dict[str, str]]:
    if not messages:
        raise ValueError("chat requests must contain at least one message")

    return [
        {"role": message.role, "content": message.content}
        for message in messages
    ]
