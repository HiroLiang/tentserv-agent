from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from enum import StrEnum
from typing import Any

from .base import BackendModel


class ChatModelKind(StrEnum):
    MLX = "mlx-chat"
    LLAMA_CPP = "llama-cpp-chat"
    TRANSFORMERS = "transformers-chat"


@dataclass(frozen=True, slots=True)
class ChatMessage:
    role: str
    content: str


@dataclass(frozen=True, slots=True)
class ChatRequest:
    messages: tuple[ChatMessage, ...]
    max_tokens: int | None = None
    temperature: float | None = None
    adapter_ref: str | None = None


@dataclass(frozen=True, slots=True)
class ChatResult:
    text: str


class ChatBackendModel(BackendModel, ABC):
    @abstractmethod
    def generate(self, request: ChatRequest) -> ChatResult:
        """Run a non-streaming chat inference request."""
        raise NotImplementedError

    @abstractmethod
    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        """Run a streaming chat inference request."""
        raise NotImplementedError


ChatModelFactory = Callable[[Any], ChatBackendModel]


def build_chat_model(kind: Any) -> ChatBackendModel:
    try:
        chat_kind = kind if isinstance(kind, ChatModelKind) else ChatModelKind(kind)
    except ValueError as exc:
        raise ValueError(f"unsupported chat model kind `{kind}`") from exc

    if chat_kind == ChatModelKind.MLX:
        from .mlx import MlxChatModel

        return MlxChatModel()
    if chat_kind == ChatModelKind.LLAMA_CPP:
        from .llama_cpp import LlamaCppChatModel

        return LlamaCppChatModel()
    if chat_kind == ChatModelKind.TRANSFORMERS:
        from .transformers import TransformersChatModel

        return TransformersChatModel()

    raise ValueError(f"unsupported chat model kind `{kind}`")
