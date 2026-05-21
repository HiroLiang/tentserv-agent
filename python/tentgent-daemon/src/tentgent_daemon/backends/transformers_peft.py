from __future__ import annotations

from collections.abc import Iterator
from contextlib import nullcontext
from dataclasses import dataclass
from threading import Thread
from typing import Any

from .base import (
    AudioSpeechBackend,
    AudioTranscriptionBackend,
    ChatBackend,
    ChatResult,
    EmbeddingBackend,
    EmbeddingResult,
    RerankBackend,
    VideoUnderstandingBackend,
    VisionChatBackend,
)
from ..runtime.adapters import StoredAdapterRecord
from ..runtime.audio import (
    AudioTranscriptionRequest,
    AudioTranscriptionResult,
    write_audio_transcription_output,
)
from ..runtime.audio_speech import (
    AudioSpeechRequest,
    AudioSpeechResult,
    write_audio_speech_output,
)
from ..runtime.chat import ChatRequest, Message
from ..runtime.embedding import EmbeddingRequest
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.rerank import RerankRequest, RerankResult, ranked_scores
from ..runtime.records import StoredModelRecord
from ..runtime.video_understanding import (
    VideoUnderstandingRequest,
    VideoUnderstandingResult,
    video_understanding_media_type,
)
from ..runtime.vision import (
    VisionChatRequest,
    VisionChatResult,
    vision_chat_media_type,
)


@dataclass(frozen=True)
class TransformersPeftDeps:
    torch: Any
    PeftModel: Any
    AutoModel: Any
    AutoModelForCausalLM: Any
    AutoModelForImageTextToText: Any
    AutoModelForSequenceClassification: Any
    AutoModelForVision2Seq: Any
    AutoProcessor: Any
    AutoTokenizer: Any
    Image: Any
    TextIteratorStreamer: Any
    pipeline: Any


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


class TransformersPeftAudioTranscriptionBackend(AudioTranscriptionBackend):
    def __init__(self) -> None:
        self._deps = _load_transformers_peft_deps()
        self._record: StoredModelRecord | None = None
        self._pipeline: Any | None = None

    def load(self, record: StoredModelRecord) -> None:
        self._record = record
        self._pipeline = self._deps.pipeline(
            "automatic-speech-recognition",
            model=str(record.variant_source_path),
            device=_asr_pipeline_device(self._deps.torch),
            chunk_length_s=30,
            stride_length_s=5,
            trust_remote_code=True,
        )

    def transcribe(
        self,
        request: AudioTranscriptionRequest,
    ) -> AudioTranscriptionResult:
        pipe = self._require_loaded()
        return_timestamps = request.timestamps or request.output_format in {"vtt", "srt"}
        kwargs: dict[str, object] = {"return_timestamps": return_timestamps}
        if request.language:
            kwargs["generate_kwargs"] = {"language": request.language}
        try:
            raw_result = pipe(str(request.input_path), **kwargs)
        except ValueError as exc:
            if request.language and _is_english_only_language_error(exc):
                raw_result = pipe(
                    str(request.input_path),
                    return_timestamps=return_timestamps,
                )
            else:
                raise
        if not isinstance(raw_result, dict):
            raw_result = {"text": str(raw_result)}
        return write_audio_transcription_output(request, raw_result)

    def release(self) -> None:
        self._record = None
        self._pipeline = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> Any:
        if self._record is None or self._pipeline is None:
            raise RuntimeError(
                "Transformers audio transcription backend is not loaded yet; "
                "call load() before transcribe()."
            )
        return self._pipeline


class TransformersPeftAudioSpeechBackend(AudioSpeechBackend):
    def __init__(self) -> None:
        self._deps = _load_transformers_peft_deps()
        self._record: StoredModelRecord | None = None
        self._pipeline: Any | None = None

    def load(self, record: StoredModelRecord) -> None:
        self._record = record
        self._pipeline = self._deps.pipeline(
            "text-to-speech",
            model=str(record.variant_source_path),
            device=_asr_pipeline_device(self._deps.torch),
            trust_remote_code=True,
        )

    def synthesize_speech(self, request: AudioSpeechRequest) -> AudioSpeechResult:
        pipe = self._require_loaded()
        kwargs: dict[str, object] = {}
        if request.language:
            kwargs["language"] = request.language
        if request.voice:
            kwargs["voice"] = request.voice
        try:
            raw_result = pipe(request.text, **kwargs)
        except TypeError as exc:
            if kwargs and "unexpected keyword" in str(exc).lower():
                unsupported = ", ".join(sorted(kwargs))
                raise ValueError(
                    "selected audio speech model does not support request option(s): "
                    f"{unsupported}"
                ) from exc
            raise
        except ValueError as exc:
            if kwargs and _is_known_tts_option_error(exc):
                unsupported = ", ".join(sorted(kwargs))
                raise ValueError(
                    "selected audio speech model rejected request option(s): "
                    f"{unsupported}. {exc}"
                ) from exc
            raise
        return write_audio_speech_output(request, raw_result)

    def release(self) -> None:
        self._record = None
        self._pipeline = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> Any:
        if self._record is None or self._pipeline is None:
            raise RuntimeError(
                "Transformers audio speech backend is not loaded yet; "
                "call load() before synthesize_speech()."
            )
        return self._pipeline


def _load_transformers_peft_deps() -> TransformersPeftDeps:
    try:
        from peft import PeftModel
        from PIL import Image
        import torch
        import transformers
        from transformers import (
            AutoModel,
            AutoModelForCausalLM,
            AutoModelForSequenceClassification,
            AutoProcessor,
            AutoTokenizer,
            TextIteratorStreamer,
            pipeline,
        )
    except ModuleNotFoundError as exc:
        if exc.name in {"PIL", "peft", "torch", "transformers"}:
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise

    return TransformersPeftDeps(
        torch=torch,
        PeftModel=PeftModel,
        AutoModel=AutoModel,
        AutoModelForCausalLM=AutoModelForCausalLM,
        AutoModelForImageTextToText=getattr(
            transformers,
            "AutoModelForImageTextToText",
            None,
        ),
        AutoModelForSequenceClassification=AutoModelForSequenceClassification,
        AutoModelForVision2Seq=getattr(transformers, "AutoModelForVision2Seq", None),
        AutoProcessor=AutoProcessor,
        AutoTokenizer=AutoTokenizer,
        Image=Image,
        TextIteratorStreamer=TextIteratorStreamer,
        pipeline=pipeline,
    )


def _detect_device(torch: Any) -> Any:
    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _asr_pipeline_device(torch: Any) -> Any:
    if torch.cuda.is_available():
        return 0
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return -1


def _is_english_only_language_error(error: ValueError) -> bool:
    message = str(error)
    return (
        "Cannot specify `task` or `language` for an English-only model" in message
    )


def _is_known_tts_option_error(error: ValueError) -> bool:
    message = str(error).lower()
    return (
        "language" in message
        or "voice" in message
        or "speaker" in message
        or "unexpected" in message
        or "unsupported" in message
    )


class TransformersPeftVisionChatBackend(VisionChatBackend):
    def __init__(self) -> None:
        self._deps = _load_transformers_peft_deps()
        self._record: StoredModelRecord | None = None
        self._processor: Any | None = None
        self._model: Any | None = None
        self._device = _detect_device(self._deps.torch)

    def load(self, record: StoredModelRecord) -> None:
        load_path = str(record.variant_source_path)
        processor = self._deps.AutoProcessor.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model_cls = _vision_model_class(self._deps)
        model = model_cls.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model.to(self._device)
        model.eval()

        self._record = record
        self._processor = processor
        self._model = model

    def generate_vision_chat(self, request: VisionChatRequest) -> VisionChatResult:
        processor, model = self._require_loaded()
        image = self._deps.Image.open(request.image_path).convert("RGB")
        prompt = _render_vision_prompt(processor, request)
        encoded = processor(
            text=prompt,
            images=[image],
            return_tensors="pt",
        )
        encoded = {
            key: value.to(self._device) if hasattr(value, "to") else value
            for key, value in encoded.items()
        }
        generate_kwargs = _vision_generate_kwargs(processor, encoded, request)

        with self._deps.torch.inference_mode():
            output_ids = model.generate(**generate_kwargs)

        prompt_length = 0
        input_ids = encoded.get("input_ids")
        if input_ids is not None:
            prompt_length = input_ids.shape[-1]
        generated_ids = output_ids[:, prompt_length:] if prompt_length else output_ids
        text = _decode_vision_output(processor, generated_ids).strip()
        return VisionChatResult(
            output_format=request.output_format,
            media_type=vision_chat_media_type(request.output_format),
            text=text,
            finish_reason="stop",
        )

    def release(self) -> None:
        self._record = None
        self._processor = None
        self._model = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._processor is None or self._model is None:
            raise RuntimeError(
                "Transformers vision chat backend is not loaded yet; "
                "call load() before generate_vision_chat()."
            )
        return self._processor, self._model


class TransformersPeftVideoUnderstandingBackend(VideoUnderstandingBackend):
    def __init__(self) -> None:
        self._deps = _load_transformers_peft_deps()
        self._record: StoredModelRecord | None = None
        self._processor: Any | None = None
        self._model: Any | None = None
        self._device = _detect_device(self._deps.torch)

    def load(self, record: StoredModelRecord) -> None:
        load_path = str(record.variant_source_path)
        processor = self._deps.AutoProcessor.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model_cls = _vision_model_class(self._deps)
        model = model_cls.from_pretrained(
            load_path,
            trust_remote_code=True,
        )
        model.to(self._device)
        model.eval()

        self._record = record
        self._processor = processor
        self._model = model

    def understand_video(
        self,
        request: VideoUnderstandingRequest,
    ) -> VideoUnderstandingResult:
        processor, model = self._require_loaded()
        frames = _sample_video_frames(self._deps, request)
        prompt = _render_video_prompt(processor, request, len(frames))
        encoded = processor(
            text=prompt,
            images=frames,
            return_tensors="pt",
        )
        encoded = {
            key: value.to(self._device) if hasattr(value, "to") else value
            for key, value in encoded.items()
        }
        generate_kwargs = _video_generate_kwargs(processor, encoded, request)

        with self._deps.torch.inference_mode():
            output_ids = model.generate(**generate_kwargs)

        prompt_length = 0
        input_ids = encoded.get("input_ids")
        if input_ids is not None:
            prompt_length = input_ids.shape[-1]
        generated_ids = output_ids[:, prompt_length:] if prompt_length else output_ids
        text = _decode_vision_output(processor, generated_ids).strip()
        return VideoUnderstandingResult(
            output_format=request.output_format,
            media_type=video_understanding_media_type(request.output_format),
            text=text,
            finish_reason="stop",
            sampled_frames=len(frames),
        )

    def release(self) -> None:
        self._record = None
        self._processor = None
        self._model = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._processor is None or self._model is None:
            raise RuntimeError(
                "Transformers video understanding backend is not loaded yet; "
                "call load() before understand_video()."
            )
        return self._processor, self._model


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


def _vision_model_class(deps: TransformersPeftDeps) -> Any:
    for candidate in (
        deps.AutoModelForImageTextToText,
        deps.AutoModelForVision2Seq,
        deps.AutoModelForCausalLM,
    ):
        if candidate is not None:
            return candidate
    raise RuntimeError(
        "Transformers does not provide a supported vision-chat auto model class"
    )


def _render_vision_prompt(processor: Any, request: VisionChatRequest) -> str:
    messages: list[dict[str, object]] = []
    if request.system_prompt:
        messages.append(
            {
                "role": "system",
                "content": [{"type": "text", "text": request.system_prompt}],
            }
        )
    messages.append(
        {
            "role": "user",
            "content": [
                {"type": "image"},
                {"type": "text", "text": request.prompt},
            ],
        }
    )

    apply_chat_template = getattr(processor, "apply_chat_template", None)
    if callable(apply_chat_template):
        return str(
            apply_chat_template(
                messages,
                tokenize=False,
                add_generation_prompt=True,
            )
        )

    if request.system_prompt:
        return f"{request.system_prompt.strip()}\n\n{request.prompt.strip()}"
    return request.prompt.strip()


def _render_video_prompt(
    processor: Any,
    request: VideoUnderstandingRequest,
    frame_count: int,
) -> str:
    content: list[dict[str, object]] = [
        {"type": "image"} for _ in range(max(frame_count, 1))
    ]
    content.append({"type": "text", "text": request.prompt})
    messages: list[dict[str, object]] = []
    if request.system_prompt:
        messages.append(
            {
                "role": "system",
                "content": [{"type": "text", "text": request.system_prompt}],
            }
        )
    messages.append({"role": "user", "content": content})

    apply_chat_template = getattr(processor, "apply_chat_template", None)
    if callable(apply_chat_template):
        return str(
            apply_chat_template(
                messages,
                tokenize=False,
                add_generation_prompt=True,
            )
        )

    prefix = ""
    if request.system_prompt:
        prefix = f"{request.system_prompt.strip()}\n\n"
    return f"{prefix}{request.prompt.strip()}"


def _vision_generate_kwargs(
    processor: Any,
    encoded: dict[str, Any],
    request: VisionChatRequest,
) -> dict[str, Any]:
    max_new_tokens = request.max_tokens or 128
    temperature = 0.0 if request.temperature is None else request.temperature
    do_sample = temperature > 0
    kwargs: dict[str, Any] = {
        **encoded,
        "max_new_tokens": max_new_tokens,
        "do_sample": do_sample,
    }
    tokenizer = getattr(processor, "tokenizer", None)
    pad_token_id = getattr(tokenizer, "pad_token_id", None)
    eos_token_id = getattr(tokenizer, "eos_token_id", None)
    if pad_token_id is not None:
        kwargs["pad_token_id"] = pad_token_id
    if eos_token_id is not None:
        kwargs["eos_token_id"] = eos_token_id
    if do_sample:
        kwargs["temperature"] = temperature
    return kwargs


def _video_generate_kwargs(
    processor: Any,
    encoded: dict[str, Any],
    request: VideoUnderstandingRequest,
) -> dict[str, Any]:
    max_new_tokens = request.max_tokens or 128
    temperature = 0.0 if request.temperature is None else request.temperature
    do_sample = temperature > 0
    kwargs: dict[str, Any] = {
        **encoded,
        "max_new_tokens": max_new_tokens,
        "do_sample": do_sample,
    }
    tokenizer = getattr(processor, "tokenizer", None)
    pad_token_id = getattr(tokenizer, "pad_token_id", None)
    eos_token_id = getattr(tokenizer, "eos_token_id", None)
    if pad_token_id is not None:
        kwargs["pad_token_id"] = pad_token_id
    if eos_token_id is not None:
        kwargs["eos_token_id"] = eos_token_id
    if do_sample:
        kwargs["temperature"] = temperature
    return kwargs


def _sample_video_frames(
    deps: TransformersPeftDeps,
    request: VideoUnderstandingRequest,
) -> list[Any]:
    try:
        import cv2
    except ModuleNotFoundError as exc:
        raise missing_profile_dependency("local-model", "opencv-python") from exc

    capture = cv2.VideoCapture(str(request.video_path))
    if not capture.isOpened():
        raise RuntimeError(
            f"video decoder could not open `{request.video_path}`; verify the "
            "container/codec is supported by the installed OpenCV/FFmpeg build"
        )

    try:
        fps = float(capture.get(cv2.CAP_PROP_FPS) or 0.0)
        frame_count = int(capture.get(cv2.CAP_PROP_FRAME_COUNT) or 0)
        sample_fps = request.sampling.sample_fps or 1.0
        max_frames = request.sampling.max_frames or 32
        max_edge = request.sampling.max_frame_edge or 768
        start_seconds = request.sampling.clip_start_seconds or 0.0
        duration_seconds = request.sampling.clip_duration_seconds

        if fps > 0:
            start_frame = max(0, int(round(start_seconds * fps)))
            step = max(1, int(round(fps / sample_fps)))
            if duration_seconds is not None:
                end_frame = start_frame + max(1, int(round(duration_seconds * fps)))
            elif frame_count > 0:
                end_frame = frame_count
            else:
                end_frame = start_frame + step * max_frames
            if frame_count > 0:
                end_frame = min(end_frame, frame_count)
            positions = range(start_frame, max(start_frame + 1, end_frame), step)
            frames = _read_positioned_frames(deps, capture, cv2, positions, max_frames, max_edge)
        else:
            frames = _read_sequential_frames(deps, capture, cv2, max_frames, max_edge)
    finally:
        capture.release()

    if not frames:
        raise RuntimeError(
            f"video decoder produced no frames from `{request.video_path}`; "
            "verify the file is not empty and the codec is supported"
        )
    return frames


def _read_positioned_frames(
    deps: TransformersPeftDeps,
    capture: Any,
    cv2: Any,
    positions: range,
    max_frames: int,
    max_edge: int,
) -> list[Any]:
    frames: list[Any] = []
    for position in positions:
        if len(frames) >= max_frames:
            break
        capture.set(cv2.CAP_PROP_POS_FRAMES, position)
        ok, frame = capture.read()
        if not ok:
            continue
        frames.append(_opencv_frame_to_pil(deps, cv2, frame, max_edge))
    return frames


def _read_sequential_frames(
    deps: TransformersPeftDeps,
    capture: Any,
    cv2: Any,
    max_frames: int,
    max_edge: int,
) -> list[Any]:
    frames: list[Any] = []
    while len(frames) < max_frames:
        ok, frame = capture.read()
        if not ok:
            break
        frames.append(_opencv_frame_to_pil(deps, cv2, frame, max_edge))
    return frames


def _opencv_frame_to_pil(
    deps: TransformersPeftDeps,
    cv2: Any,
    frame: Any,
    max_edge: int,
) -> Any:
    height, width = frame.shape[:2]
    largest = max(width, height)
    if largest > max_edge:
        scale = max_edge / float(largest)
        frame = cv2.resize(
            frame,
            (max(1, int(width * scale)), max(1, int(height * scale))),
            interpolation=cv2.INTER_AREA,
        )
    rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
    return deps.Image.fromarray(rgb)


def _decode_vision_output(processor: Any, generated_ids: Any) -> str:
    batch_decode = getattr(processor, "batch_decode", None)
    if callable(batch_decode):
        return str(batch_decode(generated_ids, skip_special_tokens=True)[0])
    tokenizer = getattr(processor, "tokenizer", None)
    if tokenizer is not None and hasattr(tokenizer, "batch_decode"):
        return str(tokenizer.batch_decode(generated_ids, skip_special_tokens=True)[0])
    raise RuntimeError("vision processor cannot decode generated token ids")
