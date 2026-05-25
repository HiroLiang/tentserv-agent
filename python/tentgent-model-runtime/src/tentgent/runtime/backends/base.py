from __future__ import annotations

from abc import ABC, abstractmethod
from enum import StrEnum
from typing import TYPE_CHECKING, ClassVar


if TYPE_CHECKING:
    from .records import ModelRecord


class BackendFamily(StrEnum):
    MLX = "mlx"
    LLAMA_CPP = "llama_cpp"
    TRANSFORMERS = "transformers"
    DIFFUSERS = "diffusers"


class BackendConcurrencyPolicy(StrEnum):
    EXCLUSIVE = "exclusive"
    SHARED = "shared"


class BackendModel(ABC):
    """Base lifecycle contract for a loaded runtime backend model."""

    family: ClassVar[BackendFamily]
    concurrency_policy: ClassVar[BackendConcurrencyPolicy] = (
        BackendConcurrencyPolicy.EXCLUSIVE
    )

    @abstractmethod
    def load(self, record: ModelRecord) -> None:
        """Load model resources from the runtime-owned model record."""

    @abstractmethod
    def release(self) -> None:
        """Release loaded model resources held by this backend."""

    @property
    @abstractmethod
    def is_loaded(self) -> bool:
        """Whether this backend currently holds loaded model resources."""

    @property
    def requires_exclusive_access(self) -> bool:
        """Whether callers should serialize access to this loaded model."""
        return self.concurrency_policy == BackendConcurrencyPolicy.EXCLUSIVE
