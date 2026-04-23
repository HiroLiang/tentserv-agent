"""Backend adapters for Tentgent Python runtime."""

from ..runtime.router import BackendKind
from .base import ChatBackend
from .llama_cpp import LlamaCppChatBackend
from .mlx import MlxChatBackend
from .transformers_peft import TransformersPeftChatBackend


def create_backend(kind: BackendKind) -> ChatBackend:
    if kind == BackendKind.MLX:
        return MlxChatBackend()
    if kind == BackendKind.TRANSFORMERS_PEFT:
        return TransformersPeftChatBackend()
    if kind == BackendKind.LLAMA_CPP:
        return LlamaCppChatBackend()

    raise ValueError(f"unsupported backend kind `{kind}`")


__all__ = ["ChatBackend", "create_backend"]
