from __future__ import annotations

import inspect
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from ..audio_transcription import (
    AudioTranscriptionBackendModel,
    AudioTranscriptionOutputFormat,
    AudioTranscriptionRequest,
    AudioTranscriptionResult,
    write_audio_transcription_output,
)
from ..base import MlxBackendModel
from ..errors import missing_backend_dependency
from ..records import ModelFormat, ModelRecord


@dataclass(frozen=True, slots=True)
class _MlxAudioDeps:
    load: Callable[[str], Any]


class MlxAudioTranscriptionModel(MlxBackendModel, AudioTranscriptionBackendModel):
    def __init__(self) -> None:
        self._deps = _load_mlx_audio_deps()
        self._record: ModelRecord | None = None
        self._model: Any | None = None

    def load(self, record: ModelRecord) -> None:
        if record.primary_format != ModelFormat.MLX:
            raise ValueError(
                "MLX audio transcription model cannot load "
                f"primary_format `{record.primary_format}`"
            )

        self._model = self._deps.load(str(record.source_path))
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None and self._model is not None

    def release(self) -> None:
        self._record = None
        self._model = None

    def transcribe(
        self,
        request: AudioTranscriptionRequest,
    ) -> AudioTranscriptionResult:
        model = self._require_loaded()
        raw_result = _generate_transcription(model, request)
        return write_audio_transcription_output(request, _normalize_result(raw_result))

    def _require_loaded(self) -> Any:
        if self._record is None or self._model is None:
            raise RuntimeError(
                "MLX audio transcription model is not loaded yet; call load() first."
            )
        return self._model


def _load_mlx_audio_deps() -> _MlxAudioDeps:
    try:
        from mlx_audio.stt import load
    except ModuleNotFoundError as exc:
        if exc.name and exc.name.startswith("mlx_audio"):
            raise missing_backend_dependency("mlx-audio") from exc
        if exc.name == "mlx":
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _MlxAudioDeps(load=load)


def _generate_transcription(
    model: Any,
    request: AudioTranscriptionRequest,
) -> Any:
    generate = getattr(model, "generate", None)
    if not callable(generate):
        raise RuntimeError(
            "MLX audio STT model does not expose a callable `generate(audio)` method."
        )

    kwargs = _generate_kwargs(generate, request)
    try:
        return generate(str(request.input_path), **kwargs)
    except Exception as exc:
        if _is_missing_processor_metadata_error(exc):
            raise RuntimeError(
                "MLX audio model package is missing Hugging Face processor/tokenizer "
                "metadata required by the current mlx-audio Whisper loader. Prefer "
                "`mlx-community/whisper-tiny-asr-fp16`, or use a model repo that "
                "contains files such as `preprocessor_config.json`, `tokenizer.json`, "
                "and `generation_config.json`."
            ) from exc
        if request.language and _is_known_language_error(exc):
            retry_kwargs = {key: value for key, value in kwargs.items() if key != "language"}
            return generate(str(request.input_path), **retry_kwargs)
        if _has_timestamp_hints(kwargs) and _is_unexpected_keyword_error(exc):
            retry_kwargs = _without_timestamp_hints(kwargs)
            return generate(str(request.input_path), **retry_kwargs)
        raise


def _generate_kwargs(
    generate: Callable[..., Any],
    request: AudioTranscriptionRequest,
) -> dict[str, object]:
    candidates: dict[str, object] = {}
    if request.language:
        candidates["language"] = request.language
    if request.timestamps or request.output_format in {
        AudioTranscriptionOutputFormat.VTT,
        AudioTranscriptionOutputFormat.SRT,
    }:
        candidates["word_timestamps"] = True

    return _supported_kwargs(
        generate,
        candidates,
        language_requested=bool(request.language),
    )


def _supported_kwargs(
    func: Callable[..., Any],
    candidates: dict[str, object],
    *,
    language_requested: bool,
) -> dict[str, object]:
    try:
        signature = inspect.signature(func)
    except (TypeError, ValueError):
        return candidates

    parameters = signature.parameters
    if any(parameter.kind == inspect.Parameter.VAR_KEYWORD for parameter in parameters.values()):
        return candidates

    supported = {
        key: value
        for key, value in candidates.items()
        if key in parameters
    }
    if language_requested and "language" not in supported:
        raise ValueError(
            "MLX audio transcription runtime does not accept a `language` option "
            "for the selected model."
        )
    return supported


def _normalize_result(raw_result: Any) -> dict[str, object]:
    return {
        "text": _result_text(raw_result),
        "chunks": _result_chunks(raw_result),
    }


def _result_text(raw_result: Any) -> str:
    for key in ("text", "transcription", "result"):
        value = _value(raw_result, key)
        if value is not None:
            return str(value).strip()
    return str(raw_result).strip()


def _result_chunks(raw_result: Any) -> list[dict[str, object]]:
    chunks: list[dict[str, object]] = []
    for key in ("chunks", "segments", "sentences"):
        items = _value(raw_result, key)
        if isinstance(items, list):
            chunks.extend(_segment_chunk(item) for item in items)
    return [chunk for chunk in chunks if chunk.get("text")]


def _segment_chunk(segment: Any) -> dict[str, object]:
    text = _value(segment, "text")
    chunk: dict[str, object] = {"text": str(text).strip() if text is not None else ""}
    timestamp = _timestamp(segment)
    if timestamp is not None:
        chunk["timestamp"] = timestamp
    return chunk


def _timestamp(segment: Any) -> tuple[float, float] | None:
    raw_timestamp = _value(segment, "timestamp")
    if isinstance(raw_timestamp, (list, tuple)) and len(raw_timestamp) >= 2:
        return _timestamp_pair(raw_timestamp[0], raw_timestamp[1])
    if isinstance(raw_timestamp, dict):
        return _timestamp_pair(raw_timestamp.get("start"), raw_timestamp.get("end"))

    return _timestamp_pair(
        _first_value(segment, ("start", "start_time", "start_seconds")),
        _first_value(segment, ("end", "end_time", "end_seconds")),
    )


def _timestamp_pair(start: Any, end: Any) -> tuple[float, float] | None:
    if start is None or end is None:
        return None
    return (float(start), float(end))


def _first_value(source: Any, keys: tuple[str, ...]) -> object | None:
    for key in keys:
        value = _value(source, key)
        if value is not None:
            return value
    return None


def _value(source: Any, key: str) -> object | None:
    if isinstance(source, dict):
        return source.get(key)
    return getattr(source, key, None)


def _has_timestamp_hints(kwargs: dict[str, object]) -> bool:
    return "word_timestamps" in kwargs


def _without_timestamp_hints(kwargs: dict[str, object]) -> dict[str, object]:
    return {
        key: value
        for key, value in kwargs.items()
        if key != "word_timestamps"
    }


def _is_known_language_error(error: BaseException) -> bool:
    message = str(error).lower()
    return "language" in message and (
        "unexpected" in message
        or "unsupported" in message
        or "not accept" in message
    )


def _is_unexpected_keyword_error(error: BaseException) -> bool:
    message = str(error).lower()
    return "unexpected keyword" in message or "got an unexpected" in message


def _is_missing_processor_metadata_error(error: BaseException) -> bool:
    message = str(error).lower()
    return (
        "preprocessor_config" in message
        or "tokenizer" in message
        or "processor" in message
    )
