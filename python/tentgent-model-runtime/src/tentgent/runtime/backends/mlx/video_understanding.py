from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..errors import missing_backend_dependency
from ..records import ModelRecord
from ..video_understanding import (
    DEFAULT_MAX_TOKENS,
    VideoUnderstandingBackendModel,
    VideoUnderstandingRequest,
    VideoUnderstandingResult,
    ensure_mlx_video_model_supported,
    render_video_prompt_text,
    video_understanding_media_type,
)
from .base import MlxBackendModel, clear_mlx_cache, require_mlx_model


@dataclass(frozen=True, slots=True)
class _MlxVlmVideoDeps:
    mx: Any
    load: Any
    load_config: Any
    video_module: Any


class MlxVlmVideoUnderstandingModel(MlxBackendModel, VideoUnderstandingBackendModel):
    def __init__(self) -> None:
        self._deps = _load_mlx_vlm_video_deps()
        self._record: ModelRecord | None = None
        self._model: Any | None = None
        self._processor: Any | None = None
        self._config: Any | None = None
        self._model_type: str | None = None

    def load(self, record: ModelRecord) -> None:
        require_mlx_model(record, "MLX VLM video understanding model")

        model, processor = self._deps.load(str(record.source_path))
        config = getattr(model, "config", None)
        if config is None:
            config = self._deps.load_config(str(record.source_path))
        model_type = ensure_mlx_video_model_supported(config)
        _copy_tokenizer_chat_template(processor)

        self._record = record
        self._model = model
        self._processor = processor
        self._config = config
        self._model_type = model_type

    @property
    def is_loaded(self) -> bool:
        return (
            self._record is not None
            and self._model is not None
            and self._processor is not None
        )

    def release(self) -> None:
        self._record = None
        self._model = None
        self._processor = None
        self._config = None
        self._model_type = None
        clear_mlx_cache()

    def understand_video(
        self,
        request: VideoUnderstandingRequest,
    ) -> VideoUnderstandingResult:
        model, processor = self._require_loaded()
        if (
            request.sampling.clip_start_seconds is not None
            or request.sampling.clip_duration_seconds is not None
        ):
            raise ValueError(
                "MLX VLM video understanding does not support clip_start_seconds "
                "or clip_duration_seconds yet; pass a pre-trimmed video instead"
            )

        messages = _video_messages(request)
        apply_chat_template = getattr(processor, "apply_chat_template", None)
        if not callable(apply_chat_template):
            raise RuntimeError("MLX VLM video processor cannot apply a chat template")
        text = str(
            apply_chat_template(
                messages,
                tokenize=False,
                add_generation_prompt=True,
            )
        )
        image_inputs, video_inputs, _fps = self._deps.video_module.process_vision_info(
            messages,
            True,
        )
        inputs = processor(
            text=[text],
            images=image_inputs,
            videos=video_inputs,
            padding=True,
            return_tensors="pt",
        )

        kwargs = _mlx_video_generate_kwargs(
            self._deps.mx,
            inputs,
            request,
            video_path=str(request.video_path),
        )
        raw_result = self._deps.video_module.generate(
            model,
            processor,
            prompt=text,
            verbose=False,
            **kwargs,
        )
        return VideoUnderstandingResult(
            output_format=request.output_format,
            media_type=video_understanding_media_type(request.output_format),
            text=_generated_text(raw_result),
            finish_reason="stop",
            sampled_frames=_sampled_frame_count(video_inputs),
        )

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._model is None or self._processor is None:
            raise RuntimeError(
                "MLX VLM video understanding model is not loaded yet; call load() first."
            )
        return self._model, self._processor


def _load_mlx_vlm_video_deps() -> _MlxVlmVideoDeps:
    try:
        import mlx.core as mx
        import mlx_vlm.video_generate as video_module
        from mlx_vlm import load
        from mlx_vlm.utils import load_config
    except ModuleNotFoundError as exc:
        if exc.name and exc.name.startswith("mlx_vlm"):
            raise missing_backend_dependency("mlx-vlm") from exc
        if exc.name == "mlx":
            raise missing_backend_dependency(exc.name) from exc
        if exc.name == "cv2":
            raise missing_backend_dependency("opencv-python") from exc
        if exc.name in {"numpy", "PIL", "requests"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _MlxVlmVideoDeps(
        mx=mx,
        load=load,
        load_config=load_config,
        video_module=video_module,
    )


def _video_messages(request: VideoUnderstandingRequest) -> list[dict[str, object]]:
    content = [
        {
            "type": "video",
            "video": str(request.video_path),
            "max_pixels": (request.sampling.max_frame_edge or 768) ** 2,
            "fps": request.sampling.sample_fps or 1.0,
            "max_frames": request.sampling.max_frames or 32,
        },
        {"type": "text", "text": render_video_prompt_text(request)},
    ]
    messages: list[dict[str, object]] = []
    if request.system_prompt:
        messages.append(
            {
                "role": "system",
                "content": [{"type": "text", "text": request.system_prompt}],
            }
        )
    messages.append({"role": "user", "content": content})
    return messages


def _mlx_video_generate_kwargs(
    mx: Any,
    inputs: dict[str, Any],
    request: VideoUnderstandingRequest,
    *,
    video_path: str,
) -> dict[str, Any]:
    pixel_values = inputs.get("pixel_values_videos", inputs.get("pixel_values"))
    if pixel_values is None:
        raise ValueError("MLX VLM video processor did not return video pixel values")

    kwargs: dict[str, Any] = {
        "video": [video_path],
        "input_ids": mx.array(inputs["input_ids"]),
        "pixel_values": mx.array(pixel_values),
        "mask": mx.array(inputs["attention_mask"]),
        "temperature": 0.0 if request.temperature is None else request.temperature,
        "max_tokens": request.max_tokens or DEFAULT_MAX_TOKENS,
    }
    if inputs.get("video_grid_thw") is not None:
        kwargs["video_grid_thw"] = mx.array(inputs["video_grid_thw"])
    if inputs.get("image_grid_thw") is not None:
        kwargs["image_grid_thw"] = mx.array(inputs["image_grid_thw"])
    return kwargs


def _copy_tokenizer_chat_template(processor: Any) -> None:
    if getattr(processor, "chat_template", None) is not None:
        return
    tokenizer = getattr(processor, "tokenizer", None)
    if tokenizer is not None and getattr(tokenizer, "chat_template", None) is not None:
        processor.chat_template = tokenizer.chat_template


def _generated_text(raw_result: object) -> str:
    text = getattr(raw_result, "text", raw_result)
    normalized = str(text).strip()
    for token in ("<end_of_utterance>",):
        normalized = normalized.removesuffix(token).strip()
    return normalized


def _sampled_frame_count(video_inputs: object) -> int | None:
    if not isinstance(video_inputs, list) or not video_inputs:
        return None
    first = video_inputs[0]
    shape = getattr(first, "shape", None)
    if shape:
        return int(shape[0])
    if isinstance(first, (list, tuple)):
        return len(first)
    return None
