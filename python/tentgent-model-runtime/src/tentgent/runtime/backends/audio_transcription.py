from __future__ import annotations

import json
from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import Any

from .base import BackendModel


class AudioTranscriptionModelKind(StrEnum):
    TRANSFORMERS_ASR = "transformers-asr"
    MLX_AUDIO = "mlx-audio"


class AudioTranscriptionOutputFormat(StrEnum):
    TEXT = "text"
    JSON = "json"
    VTT = "vtt"
    SRT = "srt"


@dataclass(frozen=True, slots=True)
class AudioTranscriptionRequest:
    input_path: Path
    output_path: Path
    output_format: AudioTranscriptionOutputFormat
    language: str | None = None
    timestamps: bool = False


@dataclass(frozen=True, slots=True)
class AudioTranscriptionResult:
    output_format: AudioTranscriptionOutputFormat
    media_type: str
    output_path: Path
    total_bytes: int
    text: str | None


class AudioTranscriptionBackendModel(BackendModel, ABC):
    @abstractmethod
    def transcribe(
        self,
        request: AudioTranscriptionRequest,
    ) -> AudioTranscriptionResult:
        """Run a batch audio transcription request."""
        raise NotImplementedError


AudioTranscriptionModelFactory = Callable[[Any], AudioTranscriptionBackendModel]


def build_audio_transcription_model(kind: Any) -> AudioTranscriptionBackendModel:
    try:
        audio_kind = (
            kind
            if isinstance(kind, AudioTranscriptionModelKind)
            else AudioTranscriptionModelKind(kind)
        )
    except ValueError as exc:
        raise ValueError(f"unsupported audio transcription model kind `{kind}`") from exc

    if audio_kind == AudioTranscriptionModelKind.TRANSFORMERS_ASR:
        from .transformers import TransformersAudioTranscriptionModel

        return TransformersAudioTranscriptionModel()
    if audio_kind == AudioTranscriptionModelKind.MLX_AUDIO:
        from .mlx import MlxAudioTranscriptionModel

        return MlxAudioTranscriptionModel()

    raise ValueError(f"unsupported audio transcription model kind `{kind}`")


def normalize_audio_transcription_output_format(
    value: str | AudioTranscriptionOutputFormat,
) -> AudioTranscriptionOutputFormat:
    if isinstance(value, AudioTranscriptionOutputFormat):
        return value

    normalized = value.strip().lower()
    if normalized == "txt":
        normalized = AudioTranscriptionOutputFormat.TEXT.value
    try:
        return AudioTranscriptionOutputFormat(normalized)
    except ValueError as exc:
        expected = ", ".join(item.value for item in AudioTranscriptionOutputFormat)
        raise ValueError(
            f"unsupported audio transcription output format `{value}`; "
            f"expected one of: {expected}"
        ) from exc


def audio_transcription_media_type(
    output_format: str | AudioTranscriptionOutputFormat,
) -> str:
    normalized = normalize_audio_transcription_output_format(output_format)
    if normalized == AudioTranscriptionOutputFormat.JSON:
        return "application/json"
    if normalized == AudioTranscriptionOutputFormat.VTT:
        return "text/vtt"
    if normalized == AudioTranscriptionOutputFormat.SRT:
        return "application/x-subrip"
    return "text/plain"


def write_audio_transcription_output(
    request: AudioTranscriptionRequest,
    raw_result: dict[str, Any],
) -> AudioTranscriptionResult:
    output_format = normalize_audio_transcription_output_format(request.output_format)
    body, text = render_audio_transcription_output(raw_result, output_format)
    request.output_path.parent.mkdir(parents=True, exist_ok=True)
    request.output_path.write_bytes(body)
    return AudioTranscriptionResult(
        output_format=output_format,
        media_type=audio_transcription_media_type(output_format),
        output_path=request.output_path,
        total_bytes=len(body),
        text=text,
    )


def render_audio_transcription_output(
    raw_result: dict[str, Any],
    output_format: str | AudioTranscriptionOutputFormat,
) -> tuple[bytes, str | None]:
    normalized = normalize_audio_transcription_output_format(output_format)
    text = _result_text(raw_result)
    if normalized == AudioTranscriptionOutputFormat.JSON:
        return (
            json.dumps(
                _json_compatible(raw_result),
                ensure_ascii=False,
                indent=2,
            ).encode("utf-8"),
            text,
        )
    if normalized == AudioTranscriptionOutputFormat.VTT:
        return _render_vtt(raw_result).encode("utf-8"), text
    if normalized == AudioTranscriptionOutputFormat.SRT:
        return _render_srt(raw_result).encode("utf-8"), text
    return (text + "\n").encode("utf-8"), text


def _result_text(raw_result: dict[str, Any]) -> str:
    value = raw_result.get("text", "")
    return str(value).strip()


def _segments(
    raw_result: dict[str, Any],
    *,
    require_timestamps: bool = False,
) -> list[tuple[float, float, str]]:
    chunks = raw_result.get("chunks")
    if isinstance(chunks, list) and chunks:
        segments: list[tuple[float, float, str]] = []
        for chunk in chunks:
            if not isinstance(chunk, dict):
                continue
            text = str(chunk.get("text", "")).strip()
            if not text:
                continue
            timestamp = chunk.get("timestamp")
            if require_timestamps and not _has_timestamp_pair(timestamp):
                raise ValueError(
                    "audio transcription subtitle output requires segment timestamps, "
                    "but at least one result chunk did not include start and end times"
                )
            start, end = _timestamp_pair(timestamp)
            segments.append((start, end, text))
        if segments:
            return segments
    if require_timestamps:
        raise ValueError(
            "audio transcription subtitle output requires segment timestamps, "
            "but the runtime result did not include timestamp chunks"
        )

    text = _result_text(raw_result)
    return [(0.0, 0.001, text)] if text else []


def _has_timestamp_pair(value: object) -> bool:
    if isinstance(value, (tuple, list)) and len(value) >= 2:
        return value[0] is not None and value[1] is not None
    if isinstance(value, dict):
        return value.get("start") is not None and value.get("end") is not None
    return False


def _timestamp_pair(value: object) -> tuple[float, float]:
    start = 0.0
    end = 0.001
    if isinstance(value, (tuple, list)) and len(value) >= 2:
        start = _timestamp_seconds(value[0], 0.0)
        end = _timestamp_seconds(value[1], start + 0.001)
    elif isinstance(value, dict):
        start = _timestamp_seconds(value.get("start"), 0.0)
        end = _timestamp_seconds(value.get("end"), start + 0.001)
    if end <= start:
        end = start + 0.001
    return start, end


def _timestamp_seconds(value: object, fallback: float) -> float:
    if value is None:
        return fallback
    try:
        return max(float(value), 0.0)
    except (TypeError, ValueError):
        return fallback


def _render_vtt(raw_result: dict[str, Any]) -> str:
    lines = ["WEBVTT", ""]
    for start, end, text in _segments(raw_result, require_timestamps=True):
        lines.append(f"{_format_vtt_timestamp(start)} --> {_format_vtt_timestamp(end)}")
        lines.append(text)
        lines.append("")
    return "\n".join(lines)


def _render_srt(raw_result: dict[str, Any]) -> str:
    lines: list[str] = []
    for index, (start, end, text) in enumerate(
        _segments(raw_result, require_timestamps=True),
        start=1,
    ):
        lines.append(str(index))
        lines.append(f"{_format_srt_timestamp(start)} --> {_format_srt_timestamp(end)}")
        lines.append(text)
        lines.append("")
    return "\n".join(lines)


def _format_vtt_timestamp(seconds: float) -> str:
    return _format_timestamp(seconds, decimal_separator=".")


def _format_srt_timestamp(seconds: float) -> str:
    return _format_timestamp(seconds, decimal_separator=",")


def _format_timestamp(seconds: float, decimal_separator: str) -> str:
    milliseconds = max(round(seconds * 1000), 0)
    hours, remainder = divmod(milliseconds, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    whole_seconds, millis = divmod(remainder, 1000)
    return (
        f"{hours:02}:{minutes:02}:{whole_seconds:02}"
        f"{decimal_separator}{millis:03}"
    )


def _json_compatible(value: Any) -> Any:
    if isinstance(value, dict):
        return {str(key): _json_compatible(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_compatible(item) for item in value]
    if isinstance(value, Path):
        return str(value)
    try:
        json.dumps(value)
        return value
    except TypeError:
        return str(value)
