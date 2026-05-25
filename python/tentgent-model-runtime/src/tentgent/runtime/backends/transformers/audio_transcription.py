from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from ..audio_transcription import (
    AudioTranscriptionBackendModel,
    AudioTranscriptionOutputFormat,
    AudioTranscriptionRequest,
    AudioTranscriptionResult,
    write_audio_transcription_output,
)
from ..base import TransformersBackendModel
from ..errors import missing_backend_dependency
from ..records import ModelFormat, ModelRecord


@dataclass(frozen=True, slots=True)
class _TransformersAudioTranscriptionDeps:
    torch: Any
    pipeline: Any


class TransformersAudioTranscriptionModel(
    TransformersBackendModel,
    AudioTranscriptionBackendModel,
):
    def __init__(self) -> None:
        self._deps = _load_transformers_audio_transcription_deps()
        self._record: ModelRecord | None = None
        self._pipeline: Any | None = None

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.SAFETENSORS:
            raise ValueError(
                "Transformers audio transcription model cannot load "
                f"primary_format `{record.primary_format}`"
            )

        self._pipeline = self._deps.pipeline(
            "automatic-speech-recognition",
            model=str(record.source_path),
            device=_asr_pipeline_device(self._deps.torch),
            chunk_length_s=30,
            stride_length_s=5,
            trust_remote_code=True,
        )
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None and self._pipeline is not None

    def release(self) -> None:
        self._record = None
        self._pipeline = None
        _clear_device_cache(self._deps.torch)

    def transcribe(
        self,
        request: AudioTranscriptionRequest,
    ) -> AudioTranscriptionResult:
        pipe = self._require_loaded()
        return_timestamps = request.timestamps or request.output_format in {
            AudioTranscriptionOutputFormat.VTT,
            AudioTranscriptionOutputFormat.SRT,
        }
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

    def _require_loaded(self) -> Any:
        if self._record is None or self._pipeline is None:
            raise RuntimeError(
                "Transformers audio transcription model is not loaded yet; "
                "call load() first."
            )
        return self._pipeline


def _load_transformers_audio_transcription_deps() -> (
    _TransformersAudioTranscriptionDeps
):
    try:
        import torch
        from transformers import pipeline
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "transformers"}:
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _TransformersAudioTranscriptionDeps(torch=torch, pipeline=pipeline)


def _asr_pipeline_device(torch: Any) -> Any:
    if torch.cuda.is_available():
        return 0
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return -1


def _clear_device_cache(torch: Any) -> None:
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    if torch.backends.mps.is_available():
        torch.mps.empty_cache()


def _is_english_only_language_error(error: ValueError) -> bool:
    return "Cannot specify `task` or `language` for an English-only model" in str(error)
