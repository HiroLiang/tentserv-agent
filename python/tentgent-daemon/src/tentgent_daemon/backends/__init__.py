"""Backend adapters for Tentgent Python runtime."""

from ..runtime.router import BackendKind
from .base import AudioTranscriptionBackend, ChatBackend, EmbeddingBackend, RerankBackend


def create_backend(kind: BackendKind) -> ChatBackend:
    if kind == BackendKind.MLX:
        from .mlx import MlxChatBackend

        return MlxChatBackend()
    if kind == BackendKind.TRANSFORMERS_PEFT:
        from .transformers_peft import TransformersPeftChatBackend

        return TransformersPeftChatBackend()
    if kind == BackendKind.LLAMA_CPP:
        from .llama_cpp import LlamaCppChatBackend

        return LlamaCppChatBackend()

    raise ValueError(f"unsupported backend kind `{kind}`")


def create_embedding_backend(kind: BackendKind) -> EmbeddingBackend:
    if kind == BackendKind.TRANSFORMERS_PEFT:
        from .transformers_peft import TransformersPeftEmbeddingBackend

        return TransformersPeftEmbeddingBackend()

    raise ValueError(f"unsupported embedding backend kind `{kind}`")


def create_rerank_backend(kind: BackendKind) -> RerankBackend:
    if kind == BackendKind.TRANSFORMERS_PEFT:
        from .transformers_peft import TransformersPeftRerankBackend

        return TransformersPeftRerankBackend()

    raise ValueError(f"unsupported rerank backend kind `{kind}`")


def create_audio_transcription_backend(
    kind: BackendKind,
) -> AudioTranscriptionBackend:
    if kind == BackendKind.TRANSFORMERS_PEFT:
        from .transformers_peft import TransformersPeftAudioTranscriptionBackend

        return TransformersPeftAudioTranscriptionBackend()

    raise ValueError(f"unsupported audio transcription backend kind `{kind}`")


__all__ = [
    "ChatBackend",
    "EmbeddingBackend",
    "RerankBackend",
    "AudioTranscriptionBackend",
    "create_backend",
    "create_audio_transcription_backend",
    "create_embedding_backend",
    "create_rerank_backend",
]
