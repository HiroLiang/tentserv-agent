from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass

from ..runtime.chat import ChatRequest
from ..runtime.records import StoredModelRecord


@dataclass(frozen=True)
class ChatResult:
    text: str


class ChatBackend:
    """Minimal backend contract for the first Python chat harness."""

    def load(self, record: StoredModelRecord) -> None:
        raise NotImplementedError

    def release(self) -> None:
        """Release loaded runtime state when the server lifecycle decides to unload."""
        return None

    def generate(self, request: ChatRequest) -> ChatResult:
        raise NotImplementedError

    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        result = self.generate(request)
        if result.text:
            yield result.text
