from __future__ import annotations

from collections.abc import Iterator
from pathlib import Path

from llama_cpp import Llama

from .base import ChatBackend, ChatResult
from ..runtime.chat import ChatRequest, Message
from ..runtime.records import StoredModelRecord


class LlamaCppChatBackend(ChatBackend):
    def __init__(self) -> None:
        self._record: StoredModelRecord | None = None
        self._model: Llama | None = None

    def load(self, record: StoredModelRecord) -> None:
        if record.primary_format != "gguf":
            raise ValueError(
                f"llama.cpp backend cannot load primary_format `{record.primary_format}`"
            )

        model_path = _resolve_gguf_path(record.variant_source_path)
        self._model = Llama(
            model_path=str(model_path),
            n_ctx=2048,
            verbose=False,
        )
        self._record = record

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

    def release(self) -> None:
        self._record = None
        self._model = None

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

    def _require_loaded(self) -> Llama:
        if self._record is None or self._model is None:
            raise RuntimeError("llama.cpp backend is not loaded yet; call load() first.")
        return self._model


def _resolve_gguf_path(source_dir: Path) -> Path:
    matches = sorted(source_dir.glob("*.gguf"))
    if not matches:
        raise FileNotFoundError(f"no GGUF file found under `{source_dir}`")
    if len(matches) > 1:
        names = ", ".join(path.name for path in matches[:5])
        raise ValueError(
            "multiple GGUF files were found in the stored variant source; "
            f"this MVP expects exactly one GGUF file (found: {names})"
        )
    return matches[0]


def _render_messages(messages: tuple[Message, ...]) -> list[dict[str, str]]:
    if not messages:
        raise ValueError("chat requests must contain at least one message")

    return [
        {"role": message.role, "content": message.content}
        for message in messages
    ]
