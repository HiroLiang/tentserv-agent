from __future__ import annotations

from collections.abc import Iterator
from typing import Any

from ..chat import ChatBackendModel, ChatMessage, ChatRequest, ChatResult
from ..base import MlxBackendModel
from ..errors import missing_backend_dependency
from ..records import AdapterRecord, ModelFormat, ModelRecord


class MlxChatModel(MlxBackendModel, ChatBackendModel):
    def __init__(self) -> None:
        self._record: ModelRecord | None = None
        self._load_path: str | None = None
        self._model: Any | None = None
        self._tokenizer: Any | None = None
        self._active_adapter_ref: str | None = None

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.MLX:
            raise ValueError(
                f"MLX chat model cannot load primary_format `{record.primary_format}`"
            )

        self._record = record
        self._load_path = str(record.source_path)
        self._load_model(adapter_path=None)

    @property
    def is_loaded(self) -> bool:
        return self._record is not None and self._model is not None

    def release(self) -> None:
        self._record = None
        self._load_path = None
        self._model = None
        self._tokenizer = None
        self._active_adapter_ref = None

    def select_adapter(self, adapter: AdapterRecord | None) -> None:
        if self._record is None:
            raise RuntimeError("MLX chat model is not loaded yet; call load() first.")

        if adapter is None:
            if self._active_adapter_ref is not None:
                self._load_model(adapter_path=None)
            return

        if adapter.adapter_format != "mlx":
            raise ValueError(
                f"MLX chat model cannot load adapter_format `{adapter.adapter_format}`"
            )

        if self._active_adapter_ref == adapter.adapter_ref:
            return

        _validate_mlx_adapter_source(adapter)
        self._load_model(adapter_path=str(adapter.source_path))
        self._active_adapter_ref = adapter.adapter_ref

    def generate(self, request: ChatRequest) -> ChatResult:
        model, tokenizer = self._require_loaded()
        self._ensure_adapter_state(request)
        prompt = _render_prompt(tokenizer, request.messages)

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

    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        model, tokenizer = self._require_loaded()
        self._ensure_adapter_state(request)
        prompt = _render_prompt(tokenizer, request.messages)

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

    def _load_model(self, adapter_path: str | None) -> None:
        if self._load_path is None:
            raise RuntimeError("MLX chat model has no model path to load.")

        load, _, _, _ = _load_mlx_symbols()
        model, tokenizer = load(self._load_path, adapter_path=adapter_path)
        self._model = model
        self._tokenizer = tokenizer
        if adapter_path is None:
            self._active_adapter_ref = None

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._model is None or self._tokenizer is None:
            raise RuntimeError("MLX chat model is not loaded yet; call load() first.")
        return self._model, self._tokenizer

    def _ensure_adapter_state(self, request: ChatRequest) -> None:
        if request.adapter_ref and self._active_adapter_ref != request.adapter_ref:
            raise RuntimeError(
                f"adapter `{request.adapter_ref}` was requested but not activated"
            )
        if not request.adapter_ref and self._active_adapter_ref is not None:
            raise RuntimeError("MLX adapter is active for a base-model request")


def _render_prompt(tokenizer: Any, messages: tuple[ChatMessage, ...]) -> str:
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


def _validate_mlx_adapter_source(adapter: AdapterRecord) -> None:
    required_files = ("adapter_config.json", "adapters.safetensors")
    missing = [
        name for name in required_files if not (adapter.source_path / name).exists()
    ]
    if missing:
        label = adapter.short_ref or adapter.adapter_ref[:12]
        raise FileNotFoundError(
            f"MLX adapter `{label}` is missing {', '.join(missing)}"
        )


def _load_mlx_symbols():
    try:
        from mlx_lm import generate, load, stream_generate
        from mlx_lm.sample_utils import make_sampler
    except ModuleNotFoundError as exc:
        if exc.name in {"mlx_lm", "mlx"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return load, generate, stream_generate, make_sampler
