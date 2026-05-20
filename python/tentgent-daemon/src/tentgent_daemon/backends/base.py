from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass

from ..runtime.chat import ChatRequest
from ..runtime.embedding import EmbeddingRequest
from ..runtime.image_generation import ImageGenerationRequest, ImageGenerationResult
from ..runtime.rerank import RerankRequest, RerankResult
from ..runtime.audio import AudioTranscriptionRequest, AudioTranscriptionResult
from ..runtime.adapters import (
    AdapterExecutionNotImplementedError,
    StoredAdapterRecord,
)
from ..runtime.records import StoredModelRecord
from ..runtime.vision import VisionChatRequest, VisionChatResult


@dataclass(frozen=True)
class ChatResult:
    text: str


@dataclass(frozen=True)
class EmbeddingResult:
    vectors: list[list[float]]


class ChatBackend:
    """Minimal backend contract for the first Python chat harness."""

    def load(self, record: StoredModelRecord) -> None:
        raise NotImplementedError

    def release(self) -> None:
        """Release loaded runtime state when the server lifecycle decides to unload."""
        return None

    def select_adapter(self, adapter: StoredAdapterRecord | None) -> None:
        """Select the request adapter, or clear adapter selection for base-model chat."""
        if adapter is None:
            return
        raise AdapterExecutionNotImplementedError(
            f"adapter `{adapter.short_ref}` is recognized, but this backend has not "
            "implemented request-time adapter execution yet."
        )

    def generate(self, request: ChatRequest) -> ChatResult:
        raise NotImplementedError

    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        result = self.generate(request)
        if result.text:
            yield result.text


class EmbeddingBackend:
    """Minimal backend contract for local embedding inference."""

    def load(self, record: StoredModelRecord) -> None:
        raise NotImplementedError

    def release(self) -> None:
        """Release loaded runtime state when the server lifecycle decides to unload."""
        return None

    def embed(self, request: EmbeddingRequest) -> EmbeddingResult:
        raise NotImplementedError


class RerankBackend:
    """Minimal backend contract for local rerank inference."""

    def load(self, record: StoredModelRecord) -> None:
        raise NotImplementedError

    def release(self) -> None:
        """Release loaded runtime state when the server lifecycle decides to unload."""
        return None

    def rerank(self, request: RerankRequest) -> RerankResult:
        raise NotImplementedError


class AudioTranscriptionBackend:
    """Minimal backend contract for local batch audio transcription."""

    def load(self, record: StoredModelRecord) -> None:
        raise NotImplementedError

    def release(self) -> None:
        """Release loaded runtime state when the server lifecycle decides to unload."""
        return None

    def transcribe(
        self,
        request: AudioTranscriptionRequest,
    ) -> AudioTranscriptionResult:
        raise NotImplementedError


class VisionChatBackend:
    """Minimal backend contract for local image-plus-text inference."""

    def load(self, record: StoredModelRecord) -> None:
        raise NotImplementedError

    def release(self) -> None:
        """Release loaded runtime state when the server lifecycle decides to unload."""
        return None

    def generate_vision_chat(self, request: VisionChatRequest) -> VisionChatResult:
        raise NotImplementedError


class ImageGenerationBackend:
    """Minimal backend contract for local text-to-image inference."""

    def load(self, record: StoredModelRecord) -> None:
        raise NotImplementedError

    def release(self) -> None:
        """Release loaded runtime state when the server lifecycle decides to unload."""
        return None

    def generate_image(self, request: ImageGenerationRequest) -> ImageGenerationResult:
        raise NotImplementedError
