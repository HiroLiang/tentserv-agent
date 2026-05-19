from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_audio_transcription_backend


TEXT_FORMAT = "text"
JSON_FORMAT = "json"
VTT_FORMAT = "vtt"
SRT_FORMAT = "srt"
SUPPORTED_OUTPUT_FORMATS = {TEXT_FORMAT, JSON_FORMAT, VTT_FORMAT, SRT_FORMAT}


@dataclass(frozen=True)
class AudioTranscriptionRequest:
    model_ref: str
    input_path: Path
    output_path: Path
    output_format: str
    language: str | None = None
    timestamps: bool = False


@dataclass(frozen=True)
class AudioTranscriptionResult:
    output_format: str
    media_type: str
    output_path: Path
    total_bytes: int
    text: str | None


@dataclass(frozen=True)
class AudioTranscriptionPlan:
    request: AudioTranscriptionRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_audio_transcription_plan(
    request: AudioTranscriptionRequest,
    home: Path | None = None,
) -> AudioTranscriptionPlan:
    record = load_model_record(request.model_ref, home=home)
    if "audio-transcription" not in record.model_capabilities:
        capabilities = ", ".join(record.model_capabilities)
        raise ValueError(
            "audio transcription endpoint requires model capability "
            f"`audio-transcription`, but model `{record.model_ref}` advertises "
            f"[{capabilities}]"
        )

    input_path = request.input_path.expanduser().resolve()
    if not input_path.is_file():
        raise FileNotFoundError(f"audio input path `{input_path}` was not found")

    output_path = request.output_path.expanduser().resolve()
    output_format = normalize_audio_transcription_output_format(request.output_format)
    return AudioTranscriptionPlan(
        request=AudioTranscriptionRequest(
            model_ref=request.model_ref,
            input_path=input_path,
            output_path=output_path,
            output_format=output_format,
            language=request.language,
            timestamps=request.timestamps,
        ),
        record=record,
        backend=resolve_audio_transcription_backend(record),
        load_path=record.variant_source_path,
    )


def normalize_audio_transcription_output_format(value: str) -> str:
    normalized = value.strip().lower()
    if normalized == "txt":
        normalized = TEXT_FORMAT
    if normalized not in SUPPORTED_OUTPUT_FORMATS:
        expected = ", ".join(sorted(SUPPORTED_OUTPUT_FORMATS))
        raise ValueError(
            f"unsupported audio transcription output format `{value}`; "
            f"expected one of: {expected}"
        )
    return normalized


def audio_transcription_media_type(output_format: str) -> str:
    output_format = normalize_audio_transcription_output_format(output_format)
    if output_format == JSON_FORMAT:
        return "application/json"
    if output_format == VTT_FORMAT:
        return "text/vtt"
    if output_format == SRT_FORMAT:
        return "application/x-subrip"
    return "text/plain"


def render_audio_transcription_output(
    raw_result: dict[str, Any],
    output_format: str,
) -> tuple[bytes, str | None]:
    output_format = normalize_audio_transcription_output_format(output_format)
    text = _result_text(raw_result)
    if output_format == JSON_FORMAT:
        return (
            json.dumps(
                _json_compatible(raw_result),
                ensure_ascii=False,
                indent=2,
            ).encode("utf-8"),
            text,
        )
    if output_format == VTT_FORMAT:
        return _render_vtt(raw_result).encode("utf-8"), text
    if output_format == SRT_FORMAT:
        return _render_srt(raw_result).encode("utf-8"), text
    return (text + "\n").encode("utf-8"), text


def write_audio_transcription_output(
    request: AudioTranscriptionRequest,
    raw_result: dict[str, Any],
) -> AudioTranscriptionResult:
    body, text = render_audio_transcription_output(raw_result, request.output_format)
    request.output_path.parent.mkdir(parents=True, exist_ok=True)
    request.output_path.write_bytes(body)
    return AudioTranscriptionResult(
        output_format=normalize_audio_transcription_output_format(request.output_format),
        media_type=audio_transcription_media_type(request.output_format),
        output_path=request.output_path,
        total_bytes=len(body),
        text=text,
    )


def _result_text(raw_result: dict[str, Any]) -> str:
    value = raw_result.get("text", "")
    return str(value).strip()


def _segments(raw_result: dict[str, Any]) -> list[tuple[float, float, str]]:
    chunks = raw_result.get("chunks")
    if isinstance(chunks, list) and chunks:
        segments: list[tuple[float, float, str]] = []
        for chunk in chunks:
            if not isinstance(chunk, dict):
                continue
            text = str(chunk.get("text", "")).strip()
            if not text:
                continue
            start, end = _timestamp_pair(chunk.get("timestamp"))
            segments.append((start, end, text))
        if segments:
            return segments

    text = _result_text(raw_result)
    return [(0.0, 0.001, text)] if text else []


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
    for start, end, text in _segments(raw_result):
        lines.append(f"{_format_vtt_timestamp(start)} --> {_format_vtt_timestamp(end)}")
        lines.append(text)
        lines.append("")
    return "\n".join(lines)


def _render_srt(raw_result: dict[str, Any]) -> str:
    lines: list[str] = []
    for index, (start, end, text) in enumerate(_segments(raw_result), start=1):
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
