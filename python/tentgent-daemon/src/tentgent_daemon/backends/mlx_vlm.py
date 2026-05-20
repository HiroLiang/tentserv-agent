from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from .base import VisionChatBackend
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.records import StoredModelRecord
from ..runtime.vision import (
    VisionChatRequest,
    VisionChatResult,
    vision_chat_media_type,
)


@dataclass(frozen=True)
class MlxVlmDeps:
    load: Any
    generate: Any
    apply_chat_template: Any
    load_config: Any


class MlxVlmVisionChatBackend(VisionChatBackend):
    def __init__(self) -> None:
        self._deps = _load_mlx_vlm_deps()
        self._record: StoredModelRecord | None = None
        self._model: Any | None = None
        self._processor: Any | None = None
        self._config: Any | None = None

    def load(self, record: StoredModelRecord) -> None:
        load_path = str(record.variant_source_path)
        model, processor = self._deps.load(load_path)
        config = getattr(model, "config", None)
        if config is None:
            config = self._deps.load_config(load_path)

        self._record = record
        self._model = model
        self._processor = processor
        self._config = config

    def generate_vision_chat(self, request: VisionChatRequest) -> VisionChatResult:
        model, processor, config = self._require_loaded()
        prompt = _request_prompt(request)
        formatted_prompt = self._deps.apply_chat_template(
            processor,
            config,
            prompt,
            num_images=1,
        )
        generate_kwargs = _generate_kwargs(request)
        raw_result = self._deps.generate(
            model,
            processor,
            formatted_prompt,
            [str(request.image_path)],
            verbose=False,
            **generate_kwargs,
        )
        text = _generated_text(raw_result)
        return VisionChatResult(
            output_format=request.output_format,
            media_type=vision_chat_media_type(request.output_format),
            text=text,
            finish_reason="stop",
        )

    def release(self) -> None:
        self._record = None
        self._model = None
        self._processor = None
        self._config = None

    def _require_loaded(self) -> tuple[Any, Any, Any]:
        if self._record is None or self._model is None or self._processor is None:
            raise RuntimeError(
                "MLX VLM vision chat backend is not loaded yet; "
                "call load() before generate_vision_chat()."
            )
        return self._model, self._processor, self._config


def _load_mlx_vlm_deps() -> MlxVlmDeps:
    try:
        from mlx_vlm import generate, load
        from mlx_vlm.prompt_utils import apply_chat_template
        from mlx_vlm.utils import load_config
    except ModuleNotFoundError as exc:
        if exc.name and exc.name.startswith("mlx_vlm"):
            raise missing_profile_dependency("local-model", "mlx-vlm") from exc
        if exc.name == "mlx":
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise

    return MlxVlmDeps(
        load=load,
        generate=generate,
        apply_chat_template=apply_chat_template,
        load_config=load_config,
    )


def _request_prompt(request: VisionChatRequest) -> str:
    if request.system_prompt:
        return f"{request.system_prompt.strip()}\n\n{request.prompt.strip()}"
    return request.prompt.strip()


def _generate_kwargs(request: VisionChatRequest) -> dict[str, object]:
    kwargs: dict[str, object] = {}
    if request.max_tokens is not None:
        kwargs["max_tokens"] = request.max_tokens
    if request.temperature is not None:
        kwargs["temperature"] = request.temperature
    return kwargs


def _generated_text(raw_result: object) -> str:
    text = getattr(raw_result, "text", raw_result)
    normalized = str(text).strip()
    for token in ("<end_of_utterance>",):
        normalized = normalized.removesuffix(token).strip()
    return normalized
