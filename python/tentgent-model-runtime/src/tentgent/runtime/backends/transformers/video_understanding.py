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
    render_video_prompt_text,
    video_understanding_media_type,
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


VIDEO_UNDERSTANDING_DEVICE_ENV = "TENTGENT_VIDEO_UNDERSTANDING_DEVICE"


@dataclass(frozen=True, slots=True)
class _TransformersVideoDeps:
    torch: Any
    AutoProcessor: Any
    AutoModelForImageTextToText: Any
    AutoModelForVision2Seq: Any
    AutoModelForCausalLM: Any
    Image: Any


class TransformersVideoUnderstandingModel(
    TransformersBackendModel,
    VideoUnderstandingBackendModel,
):
    def __init__(self) -> None:
        self._deps = _load_transformers_video_deps()
        self._record: ModelRecord | None = None
        self._processor: Any | None = None
        self._model: Any | None = None
        self._device = detect_torch_device(
            self._deps.torch,
            env_var=VIDEO_UNDERSTANDING_DEVICE_ENV,
        )

    def load(self, record: ModelRecord) -> None:
        require_safetensors_model(record, "Transformers video understanding model")

        load_path = str(record.source_path)
        processor = load_transformers_component(self._deps.AutoProcessor, load_path)
        model_cls = _video_model_class(self._deps)
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
        encoded = move_batch_to_device(encoded, self._device)
        generate_kwargs = _video_generate_kwargs(processor, encoded, request)

        with self._deps.torch.inference_mode():
            output_ids = model.generate(**generate_kwargs)

        prompt_length = 0
        input_ids = encoded.get("input_ids")
        if input_ids is not None:
            prompt_length = input_ids.shape[-1]
        generated_ids = output_ids[:, prompt_length:] if prompt_length else output_ids
        text = _decode_video_output(processor, generated_ids).strip()
        return VideoUnderstandingResult(
            output_format=request.output_format,
            media_type=video_understanding_media_type(request.output_format),
            text=text,
            finish_reason="stop",
            sampled_frames=len(frames),
        )

    def _require_loaded(self) -> tuple[Any, Any]:
        if self._record is None or self._processor is None or self._model is None:
            raise RuntimeError(
                "Transformers video understanding model is not loaded yet; "
                "call load() first."
            )
        return self._processor, self._model


def _load_transformers_video_deps() -> _TransformersVideoDeps:
    try:
        import torch
        import transformers
        from PIL import Image
        from transformers import AutoModelForCausalLM, AutoProcessor
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "transformers", "PIL"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _TransformersVideoDeps(
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


def _video_model_class(deps: _TransformersVideoDeps) -> Any:
    for candidate in (
        deps.AutoModelForImageTextToText,
        deps.AutoModelForVision2Seq,
        deps.AutoModelForCausalLM,
    ):
        if candidate is not None:
            return candidate
    raise RuntimeError(
        "Transformers does not provide a supported video-understanding auto model class"
    )


def _render_video_prompt(
    processor: Any,
    request: VideoUnderstandingRequest,
    frame_count: int,
) -> str:
    content: list[dict[str, object]] = [
        {"type": "image"} for _ in range(max(frame_count, 1))
    ]
    content.append({"type": "text", "text": render_video_prompt_text(request)})
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

    if request.system_prompt:
        return f"{request.system_prompt.strip()}\n\n{render_video_prompt_text(request)}"
    return render_video_prompt_text(request)


def _video_generate_kwargs(
    processor: Any,
    encoded: dict[str, Any],
    request: VideoUnderstandingRequest,
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


def _sample_video_frames(
    deps: _TransformersVideoDeps,
    request: VideoUnderstandingRequest,
) -> list[Any]:
    try:
        import cv2
    except ModuleNotFoundError as exc:
        raise missing_backend_dependency("opencv-python") from exc

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
            frames = _read_positioned_frames(
                deps,
                capture,
                cv2,
                positions,
                max_frames,
                max_edge,
            )
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
    deps: _TransformersVideoDeps,
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
    deps: _TransformersVideoDeps,
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
    deps: _TransformersVideoDeps,
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


def _decode_video_output(processor: Any, generated_ids: Any) -> str:
    batch_decode = getattr(processor, "batch_decode", None)
    if callable(batch_decode):
        return str(batch_decode(generated_ids, skip_special_tokens=True)[0])
    tokenizer = getattr(processor, "tokenizer", None)
    if tokenizer is not None and hasattr(tokenizer, "batch_decode"):
        return str(tokenizer.batch_decode(generated_ids, skip_special_tokens=True)[0])
    raise RuntimeError("video processor cannot decode generated token ids")
