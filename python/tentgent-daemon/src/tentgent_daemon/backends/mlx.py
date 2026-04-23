from __future__ import annotations

from collections.abc import Iterator

from mlx_lm import generate, load, stream_generate
from mlx_lm.sample_utils import make_sampler
from mlx_lm.tokenizer_utils import TokenizerWrapper

from .base import ChatBackend, ChatResult
from ..runtime.chat import ChatRequest, Message
from ..runtime.records import StoredModelRecord


class MlxChatBackend(ChatBackend):
    def __init__(self) -> None:
        self._record: StoredModelRecord | None = None
        self._model = None
        self._tokenizer: TokenizerWrapper | None = None

    def load(self, record: StoredModelRecord) -> None:
        if record.primary_format != "mlx":
            raise ValueError(
                f"mlx backend cannot load primary_format `{record.primary_format}`"
            )

        model, tokenizer = load(
            str(record.variant_source_path),
        )

        self._record = record
        self._model = model
        self._tokenizer = tokenizer

    def generate(self, request: ChatRequest) -> ChatResult:
        model, tokenizer = self._require_loaded()
        prompt = _render_prompt(tokenizer, request.messages)

        if request.adapter_ref:
            raise NotImplementedError(
                "adapter_ref is not implemented yet for the MLX backend."
            )

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
        prompt = _render_prompt(tokenizer, request.messages)

        if request.adapter_ref:
            raise NotImplementedError(
                "adapter_ref is not implemented yet for the MLX backend."
            )

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

    def _require_loaded(self) -> tuple[object, TokenizerWrapper]:
        if self._record is None or self._model is None or self._tokenizer is None:
            raise RuntimeError("MLX backend is not loaded yet; call load() first.")
        return self._model, self._tokenizer


def _render_prompt(
    tokenizer: TokenizerWrapper,
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
