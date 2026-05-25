from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..audio_speech import (
    AudioSpeechBackendModel,
    AudioSpeechRequest,
    AudioSpeechResult,
    write_audio_speech_output,
)
from ..errors import missing_backend_dependency
from ..records import ModelRecord
from .base import (
    TransformersBackendModel,
    clear_torch_device_cache,
    pipeline_device,
    require_safetensors_model,
)


@dataclass(frozen=True, slots=True)
class _TransformersAudioSpeechDeps:
    torch: Any
    pipeline: Any


class TransformersAudioSpeechModel(TransformersBackendModel, AudioSpeechBackendModel):
    def __init__(self) -> None:
        self._deps = _load_transformers_audio_speech_deps()
        self._record: ModelRecord | None = None
        self._pipeline: Any | None = None

    def load(self, record: ModelRecord) -> None:
        require_safetensors_model(record, "Transformers audio speech model")

        self._pipeline = self._deps.pipeline(
            "text-to-speech",
            model=str(record.source_path),
            device=pipeline_device(self._deps.torch),
            trust_remote_code=True,
        )
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None and self._pipeline is not None

    def release(self) -> None:
        self._record = None
        self._pipeline = None
        clear_torch_device_cache(self._deps.torch)

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

    def _require_loaded(self) -> Any:
        if self._record is None or self._pipeline is None:
            raise RuntimeError(
                "Transformers audio speech model is not loaded yet; call load() first."
            )
        return self._pipeline


def _load_transformers_audio_speech_deps() -> _TransformersAudioSpeechDeps:
    try:
        import torch
        from transformers import pipeline
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "transformers"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _TransformersAudioSpeechDeps(torch=torch, pipeline=pipeline)


def _is_known_tts_option_error(error: ValueError) -> bool:
    message = str(error).lower()
    return (
        "language" in message
        or "voice" in message
        or "speaker" in message
        or "unexpected" in message
        or "unsupported" in message
    )
