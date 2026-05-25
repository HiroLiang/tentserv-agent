from __future__ import annotations

from collections.abc import Iterator
from contextlib import nullcontext
from dataclasses import dataclass
from threading import Thread
from typing import Any

from ..base import TransformersBackendModel
from ..chat import ChatBackendModel, ChatMessage, ChatRequest, ChatResult
from ..errors import missing_backend_dependency
from ..records import AdapterRecord, ModelFormat, ModelRecord


@dataclass(frozen=True, slots=True)
class _TransformersDeps:
    torch: Any
    AutoModelForCausalLM: Any
    AutoTokenizer: Any
    TextIteratorStreamer: Any


class TransformersChatModel(TransformersBackendModel, ChatBackendModel):
    def __init__(self) -> None:
        self._deps = _load_transformers_deps()
        self._record: ModelRecord | None = None
        self._tokenizer: Any | None = None
        self._model: Any | None = None
        self._loaded_adapters: dict[str, str] = {}
        self._active_adapter_ref: str | None = None
        self._device = _detect_device(self._deps.torch)

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.SAFETENSORS:
            raise ValueError(
                "Transformers chat model cannot load "
                f"primary_format `{record.primary_format}`"
            )

        load_path = str(record.source_path)
        tokenizer = self._deps.AutoTokenizer.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        if tokenizer.pad_token_id is None and tokenizer.eos_token_id is not None:
            tokenizer.pad_token = tokenizer.eos_token

        model = self._deps.AutoModelForCausalLM.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model.to(self._device)
        model.eval()

        self._record = record
        self._tokenizer = tokenizer
        self._model = model
        self._loaded_adapters = {}
        self._active_adapter_ref = None

    @property
    def is_loaded(self) -> bool:
        return (
            self._record is not None
            and self._tokenizer is not None
            and self._model is not None
        )

    def release(self) -> None:
        self._record = None
        self._tokenizer = None
        self._model = None
        self._loaded_adapters = {}
        self._active_adapter_ref = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def select_adapter(self, adapter: AdapterRecord | None) -> None:
        _, model = self._require_loaded()

        if adapter is None:
            self._active_adapter_ref = None
            return

        peft_model_class = _load_peft_model_class()
        adapter_name = self._loaded_adapters.get(adapter.adapter_ref)
        if adapter_name is None:
            adapter_name = adapter.short_ref or adapter.adapter_ref[:12]
            if isinstance(model, peft_model_class):
                model.load_adapter(
                    str(adapter.source_path),
                    adapter_name=adapter_name,
                    is_trainable=False,
                    torch_device=self._device.type,
                )
            else:
                model = peft_model_class.from_pretrained(
                    model,
                    str(adapter.source_path),
                    adapter_name=adapter_name,
                    is_trainable=False,
                )
                model.to(self._device)
                model.eval()
                self._model = model

            self._loaded_adapters[adapter.adapter_ref] = adapter_name

        peft_model = self._require_peft_model(peft_model_class)
        peft_model.set_adapter(adapter_name)
        self._active_adapter_ref = adapter.adapter_ref

    def generate(self, request: ChatRequest) -> ChatResult:
        tokenizer, model = self._require_loaded()
        generate_kwargs = self._prepare_generate_kwargs(tokenizer, request)

        with self._adapter_context(model, request), self._deps.torch.inference_mode():
            output_ids = model.generate(**generate_kwargs)

        prompt_length = generate_kwargs["input_ids"].shape[-1]
        generated_ids = output_ids[0][prompt_length:]
        text = tokenizer.decode(generated_ids, skip_special_tokens=True)
        return ChatResult(text=text.rstrip())

    def stream_generate(self, request: ChatRequest) -> Iterator[str]:
        tokenizer, model = self._require_loaded()
        generate_kwargs = self._prepare_generate_kwargs(tokenizer, request)
        streamer = self._deps.TextIteratorStreamer(
            tokenizer,
            skip_prompt=True,
            skip_special_tokens=True,
        )
        generate_kwargs["streamer"] = streamer

        error_holder: list[BaseException] = []

        def run_generation() -> None:
            try:
                with (
                    self._adapter_context(model, request),
                    self._deps.torch.inference_mode(),
                ):
                    model.generate(**generate_kwargs)
            except BaseException as exc:  # pragma: no cover - streamed surface area.
                error_holder.append(exc)

        thread = Thread(target=run_generation, daemon=True)
        thread.start()

        for chunk in streamer:
            if chunk:
                yield chunk

        thread.join()
        if error_holder:
            raise RuntimeError("streaming generation failed") from error_holder[0]

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers chat model is not loaded yet; call load() first."
            )
        return self._tokenizer, self._model

    def _require_peft_model(self, peft_model_class: Any) -> Any:
        if not isinstance(self._model, peft_model_class):
            raise RuntimeError("PEFT adapter selection did not produce a PEFT model.")
        return self._model

    def _prepare_generate_kwargs(
        self,
        tokenizer: Any,
        request: ChatRequest,
    ) -> dict[str, Any]:
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

        kwargs: dict[str, Any] = {
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

    def _adapter_context(self, model: Any, request: ChatRequest):
        if request.adapter_ref:
            return nullcontext()

        try:
            peft_model_class = _load_peft_model_class()
        except RuntimeError:
            return nullcontext()

        if isinstance(model, peft_model_class):
            return model.disable_adapter()
        return nullcontext()


def _load_transformers_deps() -> _TransformersDeps:
    try:
        import torch
        from transformers import AutoModelForCausalLM, AutoTokenizer
        from transformers import TextIteratorStreamer
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "transformers"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _TransformersDeps(
        torch=torch,
        AutoModelForCausalLM=AutoModelForCausalLM,
        AutoTokenizer=AutoTokenizer,
        TextIteratorStreamer=TextIteratorStreamer,
    )


def _load_peft_model_class() -> Any:
    try:
        from peft import PeftModel
    except ModuleNotFoundError as exc:
        if exc.name == "peft":
            raise missing_backend_dependency(exc.name) from exc
        raise
    return PeftModel


def _detect_device(torch: Any) -> Any:
    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _render_prompt(tokenizer: Any, messages: tuple[ChatMessage, ...]) -> str:
    if not messages:
        raise ValueError("chat requests must contain at least one message")

    rendered_messages = [
        {"role": message.role, "content": message.content}
        for message in messages
    ]
    apply_chat_template = getattr(tokenizer, "apply_chat_template", None)
    if callable(apply_chat_template):
        return str(
            apply_chat_template(
                rendered_messages,
                tokenize=False,
                add_generation_prompt=True,
            )
        )

    lines: list[str] = []
    for message in messages:
        role = message.role.strip().lower() or "user"
        lines.append(f"{role.capitalize()}: {message.content.strip()}")
    lines.append("Assistant:")
    return "\n\n".join(lines)
