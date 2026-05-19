from __future__ import annotations

from collections.abc import Iterator
from contextlib import nullcontext
from dataclasses import dataclass
from threading import Thread
from typing import Any

from .base import ChatBackend, ChatResult, EmbeddingBackend, EmbeddingResult, RerankBackend
from ..runtime.adapters import StoredAdapterRecord
from ..runtime.chat import ChatRequest, Message
from ..runtime.embedding import EmbeddingRequest
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.rerank import RerankRequest, RerankResult, ranked_scores
from ..runtime.records import StoredModelRecord


@dataclass(frozen=True)
class TransformersPeftDeps:
    torch: Any
    PeftModel: Any
    AutoModel: Any
    AutoModelForCausalLM: Any
    AutoModelForSequenceClassification: Any
    AutoTokenizer: Any
    TextIteratorStreamer: Any


class TransformersPeftChatBackend(ChatBackend):
    def __init__(self) -> None:
        self._deps = _load_transformers_peft_deps()
        self._record: StoredModelRecord | None = None
        self._tokenizer: Any | None = None
        self._model: Any | None = None
        self._loaded_adapters: dict[str, str] = {}
        self._active_adapter_ref: str | None = None
        self._device = _detect_device(self._deps.torch)

    def load(self, record: StoredModelRecord) -> None:
        load_path = str(record.variant_source_path)
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

    def generate(self, request: ChatRequest) -> ChatResult:
        tokenizer, model = self._require_loaded()
        generate_kwargs = self._prepare_generate_kwargs(tokenizer, request)

        with self._adapter_context(model, request), self._deps.torch.inference_mode():
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

        torch = self._deps.torch
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
            if isinstance(model, self._deps.PeftModel):
                model.load_adapter(
                    str(adapter.source_dir),
                    adapter_name=adapter_name,
                    is_trainable=False,
                    torch_device=self._device.type,
                )
            else:
                model = self._deps.PeftModel.from_pretrained(
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
        streamer = self._deps.TextIteratorStreamer(
            tokenizer,
            skip_prompt=True,
            skip_special_tokens=True,
        )
        generate_kwargs["streamer"] = streamer

        error_holder: list[BaseException] = []

        def _run_generation() -> None:
            try:
                with self._adapter_context(model, request), self._deps.torch.inference_mode():
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

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers backend is not loaded yet; call load() before generate()."
            )
        return self._tokenizer, self._model

    def _require_peft_model(self) -> Any:
        if not isinstance(self._model, self._deps.PeftModel):
            raise RuntimeError("PEFT adapter selection did not produce a PEFT model.")
        return self._model

    def _prepare_generate_kwargs(
        self,
        tokenizer: Any,
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
        model: Any,
        request: ChatRequest,
    ):
        if request.adapter_ref:
            return nullcontext()
        if isinstance(model, self._deps.PeftModel):
            return model.disable_adapter()
        return nullcontext()


class TransformersPeftEmbeddingBackend(EmbeddingBackend):
    def __init__(self) -> None:
        self._deps = _load_transformers_peft_deps()
        self._record: StoredModelRecord | None = None
        self._tokenizer: Any | None = None
        self._model: Any | None = None
        self._device = _detect_device(self._deps.torch)

    def load(self, record: StoredModelRecord) -> None:
        load_path = str(record.variant_source_path)
        tokenizer = self._deps.AutoTokenizer.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model = self._deps.AutoModel.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model.to(self._device)
        model.eval()

        self._record = record
        self._tokenizer = tokenizer
        self._model = model

    def embed(self, request: EmbeddingRequest) -> EmbeddingResult:
        tokenizer, model = self._require_loaded()
        encoded = tokenizer(
            list(request.inputs),
            padding=True,
            truncation=True,
            return_tensors="pt",
        )
        encoded = {key: value.to(self._device) for key, value in encoded.items()}

        with self._deps.torch.inference_mode():
            outputs = model(**encoded)

        token_embeddings = outputs.last_hidden_state
        attention_mask = encoded["attention_mask"].unsqueeze(-1).expand(
            token_embeddings.size()
        )
        attention_mask = attention_mask.float()
        sum_embeddings = (token_embeddings * attention_mask).sum(dim=1)
        sum_mask = attention_mask.sum(dim=1).clamp(min=1e-9)
        vectors = sum_embeddings / sum_mask
        vectors = self._deps.torch.nn.functional.normalize(vectors, p=2, dim=1)
        return EmbeddingResult(vectors=vectors.detach().cpu().tolist())

    def release(self) -> None:
        self._record = None
        self._tokenizer = None
        self._model = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers embedding backend is not loaded yet; call load() before embed()."
            )
        return self._tokenizer, self._model


class TransformersPeftRerankBackend(RerankBackend):
    def __init__(self) -> None:
        self._deps = _load_transformers_peft_deps()
        self._record: StoredModelRecord | None = None
        self._tokenizer: Any | None = None
        self._model: Any | None = None
        self._device = _detect_device(self._deps.torch)

    def load(self, record: StoredModelRecord) -> None:
        load_path = str(record.variant_source_path)
        tokenizer = self._deps.AutoTokenizer.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model = self._deps.AutoModelForSequenceClassification.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model.to(self._device)
        model.eval()

        self._record = record
        self._tokenizer = tokenizer
        self._model = model

    def rerank(self, request: RerankRequest) -> RerankResult:
        tokenizer, model = self._require_loaded()
        encoded = tokenizer(
            [request.query] * len(request.documents),
            list(request.documents),
            padding=True,
            truncation=True,
            return_tensors="pt",
        )
        encoded = {key: value.to(self._device) for key, value in encoded.items()}

        with self._deps.torch.inference_mode():
            outputs = model(**encoded)

        logits = outputs.logits
        if len(logits.shape) == 1:
            scores_tensor = logits
        elif logits.shape[-1] == 1:
            scores_tensor = logits.squeeze(-1)
        else:
            scores_tensor = logits[:, -1]
        scores = scores_tensor.detach().float().cpu().tolist()
        return ranked_scores(scores, request.top_n)

    def release(self) -> None:
        self._record = None
        self._tokenizer = None
        self._model = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._tokenizer is None or self._model is None:
            raise RuntimeError(
                "Transformers rerank backend is not loaded yet; call load() before rerank()."
            )
        return self._tokenizer, self._model


def _load_transformers_peft_deps() -> TransformersPeftDeps:
    try:
        from peft import PeftModel
        import torch
        from transformers import (
            AutoModel,
            AutoModelForCausalLM,
            AutoModelForSequenceClassification,
            AutoTokenizer,
            TextIteratorStreamer,
        )
    except ModuleNotFoundError as exc:
        if exc.name in {"peft", "torch", "transformers"}:
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise

    return TransformersPeftDeps(
        torch=torch,
        PeftModel=PeftModel,
        AutoModel=AutoModel,
        AutoModelForCausalLM=AutoModelForCausalLM,
        AutoModelForSequenceClassification=AutoModelForSequenceClassification,
        AutoTokenizer=AutoTokenizer,
        TextIteratorStreamer=TextIteratorStreamer,
    )


def _detect_device(torch: Any) -> Any:
    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


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
