from __future__ import annotations

from collections.abc import Iterator
from threading import Thread

import torch
from transformers import (
    AutoModelForCausalLM,
    AutoTokenizer,
    PreTrainedModel,
    PreTrainedTokenizerBase,
    TextIteratorStreamer,
)

from .base import ChatBackend, ChatResult
from ..runtime.chat import ChatRequest, Message
from ..runtime.records import StoredModelRecord


class TransformersPeftChatBackend(ChatBackend):
    def __init__(self) -> None:
        self._record: StoredModelRecord | None = None
        self._tokenizer: PreTrainedTokenizerBase | None = None
        self._model: PreTrainedModel | None = None
        self._device = _detect_device()

    def load(self, record: StoredModelRecord) -> None:
        load_path = str(record.variant_source_path)
        tokenizer = AutoTokenizer.from_pretrained(load_path, trust_remote_code=True)
        if tokenizer.pad_token_id is None and tokenizer.eos_token_id is not None:
            tokenizer.pad_token = tokenizer.eos_token

        model = AutoModelForCausalLM.from_pretrained(load_path, trust_remote_code=True)
        model.to(self._device)
        model.eval()

        self._record = record
        self._tokenizer = tokenizer
        self._model = model

    def generate(self, request: ChatRequest) -> ChatResult:
        tokenizer, model = self._require_loaded()
        generate_kwargs = self._prepare_generate_kwargs(tokenizer, request)

        with torch.inference_mode():
            output_ids = model.generate(**generate_kwargs)

        prompt_length = generate_kwargs["input_ids"].shape[-1]
        generated_ids = output_ids[0][prompt_length:]
        text = tokenizer.decode(generated_ids, skip_special_tokens=True)
        return ChatResult(text=text.rstrip())

    def release(self) -> None:
        self._record = None
        self._tokenizer = None
        self._model = None

        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        tokenizer, model = self._require_loaded()
        generate_kwargs = self._prepare_generate_kwargs(tokenizer, request)
        streamer = TextIteratorStreamer(
            tokenizer,
            skip_prompt=True,
            skip_special_tokens=True,
        )
        generate_kwargs["streamer"] = streamer

        error_holder: list[BaseException] = []

        def _run_generation() -> None:
            try:
                with torch.inference_mode():
                    model.generate(**generate_kwargs)
            except BaseException as exc:  # pragma: no cover - streamed surface area.
                error_holder.append(exc)

        thread = Thread(target=_run_generation, daemon=True)
        thread.start()

        for chunk in streamer:
            if chunk:
                yield chunk

        thread.join()
        if error_holder:
            raise RuntimeError("streaming generation failed") from error_holder[0]

    def _require_loaded(
        self,
    ) -> tuple[PreTrainedTokenizerBase, PreTrainedModel]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers backend is not loaded yet; call load() before generate()."
            )
        return self._tokenizer, self._model

    def _prepare_generate_kwargs(
        self,
        tokenizer: PreTrainedTokenizerBase,
        request: ChatRequest,
    ) -> dict[str, object]:
        if request.adapter_ref:
            raise NotImplementedError(
                "adapter_ref is not implemented yet for the transformers backend."
            )

        prompt = _render_prompt(tokenizer, request.messages)
        encoded = tokenizer(prompt, return_tensors="pt")
        encoded = {key: value.to(self._device) for key, value in encoded.items()}

        max_new_tokens = request.max_tokens or 128
        temperature = 0.0 if request.temperature is None else request.temperature
        do_sample = temperature > 0

        kwargs: dict[str, object] = {
            **encoded,
            "max_new_tokens": max_new_tokens,
            "do_sample": do_sample,
            "pad_token_id": tokenizer.pad_token_id,
        }
        if tokenizer.eos_token_id is not None:
            kwargs["eos_token_id"] = tokenizer.eos_token_id
        if do_sample:
            kwargs["temperature"] = temperature
        return kwargs


def _detect_device() -> torch.device:
    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _render_prompt(
    tokenizer: PreTrainedTokenizerBase,
    messages: tuple[Message, ...],
) -> str:
    if not messages:
        raise ValueError("chat requests must contain at least one message")

    rendered_messages = [
        {"role": message.role, "content": message.content}
        for message in messages
    ]

    chat_template = getattr(tokenizer, "chat_template", None)
    if chat_template:
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
