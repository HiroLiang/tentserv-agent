from __future__ import annotations

from collections.abc import Iterator
from typing import Any

from .base import ChatBackend, ChatResult
from ..runtime.adapters import StoredAdapterRecord
from ..runtime.chat import ChatRequest, Message
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.records import StoredModelRecord


class MlxChatBackend(ChatBackend):
    def __init__(self) -> None:
        self._record: StoredModelRecord | None = None
        self._load_path: str | None = None
        self._model = None
        self._tokenizer = None
        self._active_adapter_ref: str | None = None

    def load(self, record: StoredModelRecord) -> None:
        if record.primary_format != "mlx":
            raise ValueError(
                f"mlx backend cannot load primary_format `{record.primary_format}`"
            )

        self._record = record
        self._load_path = str(record.variant_source_path)
        self._load_model(adapter_path=None)

    def select_adapter(self, adapter: StoredAdapterRecord | None) -> None:
        if self._record is None:
            raise RuntimeError("MLX backend is not loaded yet; call load() first.")

        if adapter is None:
            if self._active_adapter_ref is not None:
                self._load_model(adapter_path=None)
            return

        if adapter.adapter_format != "mlx":
            raise ValueError(
                f"MLX backend cannot load adapter_format `{adapter.adapter_format}`"
            )

        if self._active_adapter_ref == adapter.adapter_ref:
            return

        _validate_mlx_adapter_source(adapter)
        self._load_model(adapter_path=str(adapter.source_dir))
        self._active_adapter_ref = adapter.adapter_ref

    def _load_model(self, adapter_path: str | None) -> None:
        if self._load_path is None:
            raise RuntimeError("MLX backend has no model path to load.")

        load, _, _, _ = _load_mlx_symbols()
        model, tokenizer = load(self._load_path, adapter_path=adapter_path)

        self._model = model
        self._tokenizer = tokenizer
        if adapter_path is None:
            self._active_adapter_ref = None

    def generate(self, request: ChatRequest) -> ChatResult:
        model, tokenizer = self._require_loaded()
        prompt = _render_prompt(tokenizer, request.messages)

        self._ensure_adapter_state(request)
        _, generate, _, make_sampler = _load_mlx_symbols()

        text = generate(
            model,
            tokenizer,
            prompt=prompt,
            verbose=False,
            max_tokens=request.max_tokens or 128,
            sampler=make_sampler(
                temp=0.0 if request.temperature is None else request.temperature
            ),
        )
        return ChatResult(text=text.rstrip())

    def release(self) -> None:
        self._record = None
        self._load_path = None
        self._model = None
        self._tokenizer = None
        self._active_adapter_ref = None

    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        model, tokenizer = self._require_loaded()
        prompt = _render_prompt(tokenizer, request.messages)

        self._ensure_adapter_state(request)
        _, _, stream_generate, make_sampler = _load_mlx_symbols()

        for response in stream_generate(
            model,
            tokenizer,
            prompt=prompt,
            max_tokens=request.max_tokens or 128,
            sampler=make_sampler(
                temp=0.0 if request.temperature is None else request.temperature
            ),
        ):
            if response.text:
                yield response.text

    def _require_loaded(self) -> tuple[object, object]:
        if self._record is None or self._model is None or self._tokenizer is None:
            raise RuntimeError("MLX backend is not loaded yet; call load() first.")
        return self._model, self._tokenizer

    def _ensure_adapter_state(self, request: ChatRequest) -> None:
        if request.adapter_ref and self._active_adapter_ref != request.adapter_ref:
            raise RuntimeError(
                f"adapter `{request.adapter_ref}` was requested but not activated"
            )
        if not request.adapter_ref and self._active_adapter_ref is not None:
            raise RuntimeError("MLX adapter is active for a base-model request")


def _render_prompt(
    tokenizer: Any,
    messages: tuple[Message, ...],
) -> str:
    if not messages:
        raise ValueError("chat requests must contain at least one message")

    rendered_messages = [
        {"role": message.role, "content": message.content}
        for message in messages
    ]

    if getattr(tokenizer, "has_chat_template", False):
        return tokenizer.apply_chat_template(
            rendered_messages,
            tokenize=False,
            add_generation_prompt=True,
        )

    lines: list[str] = []
    for message in messages:
        role = message.role.strip().lower() or "user"
        lines.append(f"{role.capitalize()}: {message.content.strip()}")
    lines.append("Assistant:")
    return "\n\n".join(lines)


def _validate_mlx_adapter_source(adapter: StoredAdapterRecord) -> None:
    required_files = ("adapter_config.json", "adapters.safetensors")
    missing = [
        name for name in required_files if not (adapter.source_dir / name).exists()
    ]
    if missing:
        raise FileNotFoundError(
            f"MLX adapter `{adapter.short_ref}` is missing {', '.join(missing)}"
        )


def _load_mlx_symbols():
    try:
        from mlx_lm import generate, load, stream_generate
        from mlx_lm.sample_utils import make_sampler
    except ModuleNotFoundError as exc:
        if exc.name in {"mlx_lm", "mlx"}:
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise

    return load, generate, stream_generate, make_sampler
