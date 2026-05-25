from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..errors import missing_backend_dependency
from ..records import ModelRecord
from ..vision_chat import (
    DEFAULT_MAX_TOKENS,
    VisionChatBackendModel,
    VisionChatRequest,
    VisionChatResult,
    vision_chat_media_type,
)
from .base import (
    TransformersBackendModel,
    clear_torch_device_cache,
    detect_torch_device,
    load_transformers_component,
    load_transformers_model,
    move_batch_to_device,
    require_safetensors_model,
)


VISION_CHAT_DEVICE_ENV = "TENTGENT_VISION_CHAT_DEVICE"


@dataclass(frozen=True, slots=True)
class _TransformersVisionDeps:
    torch: Any
    AutoProcessor: Any
    AutoModelForImageTextToText: Any
    AutoModelForVision2Seq: Any
    AutoModelForCausalLM: Any
    Image: Any


class TransformersVisionChatModel(
    TransformersBackendModel,
    VisionChatBackendModel,
):
    def __init__(self) -> None:
        self._deps = _load_transformers_vision_deps()
        self._record: ModelRecord | None = None
        self._processor: Any | None = None
        self._model: Any | None = None
        self._device = detect_torch_device(
            self._deps.torch,
            env_var=VISION_CHAT_DEVICE_ENV,
        )

    def load(self, record: ModelRecord) -> None:
        require_safetensors_model(record, "Transformers vision chat model")

        load_path = str(record.source_path)
        processor = load_transformers_component(self._deps.AutoProcessor, load_path)
        model_cls = _vision_model_class(self._deps)
        model = load_transformers_model(
            model_cls,
            load_path,
            self._device,
        )

        self._record = record
        self._processor = processor
        self._model = model

    @property
    def is_loaded(self) -> bool:
        return (
            self._record is not None
            and self._processor is not None
            and self._model is not None
        )

    def release(self) -> None:
        self._record = None
        self._processor = None
        self._model = None
        clear_torch_device_cache(self._deps.torch)

    def generate_vision_chat(self, request: VisionChatRequest) -> VisionChatResult:
        processor, model = self._require_loaded()
        with self._deps.Image.open(request.image_path) as image:
            image = image.convert("RGB")
            prompt = _render_vision_prompt(processor, request)
            encoded = processor(
                text=prompt,
                images=[image],
                return_tensors="pt",
            )

        encoded = move_batch_to_device(encoded, self._device)
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

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._processor is None or self._model is None:
            raise RuntimeError(
                "Transformers vision chat model is not loaded yet; call load() first."
            )
        return self._processor, self._model


def _load_transformers_vision_deps() -> _TransformersVisionDeps:
    try:
        import torch
        import transformers
        from PIL import Image
        from transformers import AutoModelForCausalLM, AutoProcessor
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "transformers", "PIL"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _TransformersVisionDeps(
        torch=torch,
        AutoProcessor=AutoProcessor,
        AutoModelForImageTextToText=getattr(
            transformers,
            "AutoModelForImageTextToText",
            None,
        ),
        AutoModelForVision2Seq=getattr(transformers, "AutoModelForVision2Seq", None),
        AutoModelForCausalLM=AutoModelForCausalLM,
        Image=Image,
    )


def _vision_model_class(deps: _TransformersVisionDeps) -> Any:
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


def _vision_generate_kwargs(
    processor: Any,
    encoded: dict[str, Any],
    request: VisionChatRequest,
) -> dict[str, Any]:
    max_new_tokens = request.max_tokens or DEFAULT_MAX_TOKENS
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


def _decode_vision_output(processor: Any, generated_ids: Any) -> str:
    batch_decode = getattr(processor, "batch_decode", None)
    if callable(batch_decode):
        return str(batch_decode(generated_ids, skip_special_tokens=True)[0])
    tokenizer = getattr(processor, "tokenizer", None)
    if tokenizer is not None and hasattr(tokenizer, "batch_decode"):
        return str(tokenizer.batch_decode(generated_ids, skip_special_tokens=True)[0])
    raise RuntimeError("vision processor cannot decode generated token ids")

