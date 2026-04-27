from __future__ import annotations

from collections.abc import Iterator
from contextlib import nullcontext
from threading import Thread

from peft import PeftModel
import torch
from transformers import (
    AutoModelForCausalLM,
    AutoTokenizer,
    PreTrainedModel,
    PreTrainedTokenizerBase,
    TextIteratorStreamer,
)

from .base import ChatBackend, ChatResult
from ..runtime.adapters import StoredAdapterRecord
from ..runtime.chat import ChatRequest, Message
from ..runtime.records import StoredModelRecord


class TransformersPeftChatBackend(ChatBackend):
    def __init__(self) -> None:
        self._record: StoredModelRecord | None = None
        self._tokenizer: PreTrainedTokenizerBase | None = None
        self._model: PreTrainedModel | PeftModel | None = None
        self._loaded_adapters: dict[str, str] = {}
        self._active_adapter_ref: str | None = None
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
        self._loaded_adapters = {}
        self._active_adapter_ref = None

    def generate(self, request: ChatRequest) -> ChatResult:
        tokenizer, model = self._require_loaded()
        generate_kwargs = self._prepare_generate_kwargs(tokenizer, request)

        with self._adapter_context(model, request), torch.inference_mode():
            output_ids = model.generate(**generate_kwargs)

        prompt_length = generate_kwargs["input_ids"].shape[-1]
        generated_ids = output_ids[0][prompt_length:]
        text = tokenizer.decode(generated_ids, skip_special_tokens=True)
        return ChatResult(text=text.rstrip())

    def release(self) -> None:
        self._record = None
        self._tokenizer = None
        self._model = None
        self._loaded_adapters = {}
        self._active_adapter_ref = None

        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def select_adapter(self, adapter: StoredAdapterRecord | None) -> None:
        _, model = self._require_loaded()

        if adapter is None:
            self._active_adapter_ref = None
            return

        adapter_name = self._loaded_adapters.get(adapter.adapter_ref)
        if adapter_name is None:
            adapter_name = adapter.short_ref
            if isinstance(model, PeftModel):
                model.load_adapter(
                    str(adapter.source_dir),
                    adapter_name=adapter_name,
                    is_trainable=False,
                    torch_device=self._device.type,
                )
            else:
                model = PeftModel.from_pretrained(
                    model,
                    str(adapter.source_dir),
                    adapter_name=adapter_name,
                    is_trainable=False,
                )
                model.to(self._device)
                model.eval()
                self._model = model

            self._loaded_adapters[adapter.adapter_ref] = adapter_name

        peft_model = self._require_peft_model()
        peft_model.set_adapter(adapter_name)
        self._active_adapter_ref = adapter.adapter_ref

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
                with self._adapter_context(model, request), torch.inference_mode():
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
    ) -> tuple[PreTrainedTokenizerBase, PreTrainedModel | PeftModel]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers backend is not loaded yet; call load() before generate()."
            )
        return self._tokenizer, self._model

    def _require_peft_model(self) -> PeftModel:
        if not isinstance(self._model, PeftModel):
            raise RuntimeError("PEFT adapter selection did not produce a PEFT model.")
        return self._model

    def _prepare_generate_kwargs(
        self,
        tokenizer: PreTrainedTokenizerBase,
        request: ChatRequest,
    ) -> dict[str, object]:
        if request.adapter_ref and self._active_adapter_ref != request.adapter_ref:
            raise RuntimeError(
                f"adapter `{request.adapter_ref}` was requested but not activated"
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

    def _adapter_context(
        self,
        model: PreTrainedModel | PeftModel,
        request: ChatRequest,
    ):
        if request.adapter_ref:
            return nullcontext()
        if isinstance(model, PeftModel):
            return model.disable_adapter()
        return nullcontext()


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
