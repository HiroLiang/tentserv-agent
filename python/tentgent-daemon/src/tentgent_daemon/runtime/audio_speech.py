from __future__ import annotations

import os
import struct
import wave
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_audio_speech_backend


WAV_FORMAT = "wav"
SUPPORTED_OUTPUT_FORMATS = {WAV_FORMAT}
MAX_TEXT_BYTES_ENV = "TENTGENT_AUDIO_SPEECH_MAX_TEXT_BYTES"
DEFAULT_MAX_TEXT_BYTES = 64 * 1024


@dataclass(frozen=True)
class AudioSpeechRequest:
    model_ref: str
    text: str
    output_path: Path
    output_format: str = WAV_FORMAT
    language: str | None = None
    voice: str | None = None


@dataclass(frozen=True)
class AudioSpeechResult:
    output_format: str
    media_type: str
    output_path: Path
    total_bytes: int
    sample_rate: int | None


@dataclass(frozen=True)
class AudioSpeechPlan:
    request: AudioSpeechRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_audio_speech_plan(
    request: AudioSpeechRequest,
    home: Path | None = None,
) -> AudioSpeechPlan:
    record = load_model_record(request.model_ref, home=home)
    if "audio-speech" not in record.model_capabilities:
        capabilities = ", ".join(record.model_capabilities)
        raise ValueError(
            "audio speech endpoint requires model capability "
            f"`audio-speech`, but model `{record.model_ref}` advertises "
            f"[{capabilities}]"
        )

    text = validate_audio_speech_text(request.text)
    output_path = request.output_path.expanduser().resolve()
    output_format = normalize_audio_speech_output_format(request.output_format)
    return AudioSpeechPlan(
        request=AudioSpeechRequest(
            model_ref=request.model_ref,
            text=text,
            output_path=output_path,
            output_format=output_format,
            language=_optional_text(request.language),
            voice=_optional_text(request.voice),
        ),
        record=record,
        backend=resolve_audio_speech_backend(record),
        load_path=record.variant_source_path,
    )


def validate_audio_speech_text(value: str) -> str:
    text = value.strip()
    if not text:
        raise ValueError("audio speech text must not be empty")
    max_bytes = audio_speech_max_text_bytes()
    byte_len = len(text.encode("utf-8"))
    if byte_len > max_bytes:
        raise ValueError(
            f"audio speech text is {byte_len} bytes, which exceeds the "
            f"{max_bytes} byte limit"
        )
    return text


def audio_speech_max_text_bytes() -> int:
    raw = os.environ.get(MAX_TEXT_BYTES_ENV, "").strip()
    if not raw:
        return DEFAULT_MAX_TEXT_BYTES
    try:
        value = int(raw)
    except ValueError as exc:
        raise ValueError(f"{MAX_TEXT_BYTES_ENV} must be a positive integer") from exc
    if value <= 0:
        raise ValueError(f"{MAX_TEXT_BYTES_ENV} must be a positive integer")
    return value


def normalize_audio_speech_output_format(value: str) -> str:
    normalized = value.strip().lower()
    if normalized == "wave":
        normalized = WAV_FORMAT
    if normalized not in SUPPORTED_OUTPUT_FORMATS:
        expected = ", ".join(sorted(SUPPORTED_OUTPUT_FORMATS))
        raise ValueError(
            f"unsupported audio speech output format `{value}`; "
            f"expected one of: {expected}"
        )
    return normalized


def audio_speech_media_type(output_format: str) -> str:
    output_format = normalize_audio_speech_output_format(output_format)
    if output_format == WAV_FORMAT:
        return "audio/wav"
    raise AssertionError(f"unhandled audio speech output format: {output_format}")


def write_audio_speech_output(
    request: AudioSpeechRequest,
    raw_result: Any,
) -> AudioSpeechResult:
    output_format = normalize_audio_speech_output_format(request.output_format)
    if output_format != WAV_FORMAT:
        raise ValueError(f"unsupported audio speech output format `{output_format}`")
    sample_rate = _sample_rate(raw_result)
    samples = _mono_pcm16_samples(_audio_payload(raw_result))
    request.output_path.parent.mkdir(parents=True, exist_ok=True)
    write_pcm16_wav(request.output_path, samples, sample_rate)
    total_bytes = request.output_path.stat().st_size
    return AudioSpeechResult(
        output_format=output_format,
        media_type=audio_speech_media_type(output_format),
        output_path=request.output_path,
        total_bytes=total_bytes,
        sample_rate=sample_rate,
    )


def write_pcm16_wav(path: Path, samples: list[int], sample_rate: int) -> None:
    if sample_rate <= 0:
        raise ValueError(f"audio speech sample rate must be positive; got {sample_rate}")
    if not samples:
        raise ValueError("audio speech runtime produced no audio samples")
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(sample_rate)
        handle.writeframes(b"".join(struct.pack("<h", sample) for sample in samples))


def _audio_payload(raw_result: Any) -> Any:
    for key in ("audio", "waveform", "array"):
        value = _value(raw_result, key)
        if value is not None:
            return value
    raise ValueError(
        "audio speech runtime result did not include an `audio` or `waveform` payload"
    )


def _sample_rate(raw_result: Any) -> int:
    for key in ("sampling_rate", "sample_rate", "rate"):
        value = _value(raw_result, key)
        if value is not None:
            sample_rate = int(value)
            if sample_rate <= 0:
                raise ValueError(
                    f"audio speech runtime returned invalid sample rate {sample_rate}"
                )
            return sample_rate
    raise ValueError("audio speech runtime result did not include a sample rate")


def _mono_pcm16_samples(payload: Any) -> list[int]:
    values = list(_flatten_audio_values(_tolist(payload)))
    return [_sample_to_int16(value) for value in values]


def _tolist(value: Any) -> Any:
    detach = getattr(value, "detach", None)
    if callable(detach):
        value = detach()
    cpu = getattr(value, "cpu", None)
    if callable(cpu):
        value = cpu()
    numpy = getattr(value, "numpy", None)
    if callable(numpy):
        value = numpy()
    tolist = getattr(value, "tolist", None)
    if callable(tolist):
        return tolist()
    return value


def _flatten_audio_values(value: Any):
    if isinstance(value, bytes):
        for index in range(0, len(value), 2):
            if index + 1 < len(value):
                yield struct.unpack("<h", value[index : index + 2])[0]
        return
    if isinstance(value, (str, bytearray)):
        raise ValueError("audio speech runtime returned an unsupported audio payload")
    if isinstance(value, (list, tuple)):
        for item in value:
            yield from _flatten_audio_values(item)
        return
    yield value


def _sample_to_int16(value: Any) -> int:
    sample = float(value)
    if -1.0 <= sample <= 1.0:
        sample = sample * 32767.0
    return max(min(round(sample), 32767), -32768)


def _value(source: Any, key: str) -> object | None:
    if isinstance(source, dict):
        return source.get(key)
    return getattr(source, key, None)


def _optional_text(value: str | None) -> str | None:
    if value is None:
        return None
    stripped = value.strip()
    return stripped or None
