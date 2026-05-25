from __future__ import annotations

import inspect
from collections.abc import Callable, Iterable
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
from .base import MlxBackendModel, clear_mlx_cache, require_mlx_model


@dataclass(frozen=True, slots=True)
class _MlxAudioTtsDeps:
    load_model: Callable[..., Any]
    get_model_name_parts: Callable[[str], list[str]]


class MlxAudioSpeechModel(MlxBackendModel, AudioSpeechBackendModel):
    def __init__(self) -> None:
        self._deps = _load_mlx_audio_tts_deps()
        self._record: ModelRecord | None = None
        self._model: Any | None = None

    def load(self, record: ModelRecord) -> None:
        require_mlx_model(record, "MLX audio speech model")
        load_kwargs = _load_model_kwargs(record, self._deps)
        try:
            self._model = self._deps.load_model(record.source_path, **load_kwargs)
        except Exception as exc:
            raise ValueError(
                f"failed to load MLX audio speech model `{record.model_ref}` "
                f"from `{record.source_path}`: {exc}"
            ) from exc
        self._record = record

    @property
    def is_loaded(self) -> bool:
        return self._record is not None and self._model is not None

    def release(self) -> None:
        self._record = None
        self._model = None
        clear_mlx_cache()

    def synthesize_speech(self, request: AudioSpeechRequest) -> AudioSpeechResult:
        model = self._require_loaded()
        raw_result = _generate_speech(model, request)
        return write_audio_speech_output(request, raw_result)

    def _require_loaded(self) -> Any:
        if self._record is None or self._model is None:
            raise RuntimeError("MLX audio speech model is not loaded yet; call load() first.")
        return self._model


def _load_mlx_audio_tts_deps() -> _MlxAudioTtsDeps:
    try:
        from mlx_audio.tts.utils import load_model
        from mlx_audio.utils import get_model_name_parts
    except ModuleNotFoundError as exc:
        if exc.name and exc.name.startswith("mlx_audio"):
            raise missing_backend_dependency("mlx-audio") from exc
        if exc.name == "mlx":
            raise missing_backend_dependency(exc.name) from exc
        raise

    return _MlxAudioTtsDeps(
        load_model=load_model,
        get_model_name_parts=get_model_name_parts,
    )


def _load_model_kwargs(
    record: ModelRecord,
    deps: _MlxAudioTtsDeps,
) -> dict[str, object]:
    kwargs: dict[str, object] = {}
    if record.source_repo:
        kwargs["model_name_parts"] = deps.get_model_name_parts(record.source_repo)
    return kwargs


def _generate_speech(
    model: Any,
    request: AudioSpeechRequest,
) -> dict[str, object]:
    generate = getattr(model, "generate", None)
    if not callable(generate):
        raise RuntimeError(
            "MLX audio TTS model does not expose a callable `generate(text)` method."
        )

    kwargs = _generate_kwargs(generate, request)
    raw_result = _call_generate(generate, request.text, kwargs)
    return _normalize_speech_result(raw_result, model=model)


def _generate_kwargs(
    generate: Callable[..., Any],
    request: AudioSpeechRequest,
) -> dict[str, object]:
    try:
        signature = inspect.signature(generate)
    except (TypeError, ValueError):
        return _fallback_kwargs(request)

    parameters = signature.parameters
    has_var_keyword = any(
        parameter.kind == inspect.Parameter.VAR_KEYWORD
        for parameter in parameters.values()
    )
    kwargs: dict[str, object] = {}

    if request.voice:
        if "voice" in parameters or has_var_keyword:
            kwargs["voice"] = request.voice
        else:
            raise ValueError(
                "MLX audio speech runtime does not accept a `voice` option "
                "for the selected model."
            )

    if request.language:
        if "language" in parameters:
            kwargs["language"] = request.language
        elif "lang_code" in parameters or has_var_keyword:
            kwargs["lang_code"] = request.language
        else:
            raise ValueError(
                "MLX audio speech runtime does not accept a `language` option "
                "for the selected model."
            )

    return kwargs


def _fallback_kwargs(request: AudioSpeechRequest) -> dict[str, object]:
    kwargs: dict[str, object] = {}
    if request.voice:
        kwargs["voice"] = request.voice
    if request.language:
        kwargs["lang_code"] = request.language
    return kwargs


def _call_generate(
    generate: Callable[..., Any],
    text: str,
    kwargs: dict[str, object],
) -> Any:
    try:
        if _supports_text_keyword(generate):
            return generate(text=text, **kwargs)
        return generate(text, **kwargs)
    except TypeError as exc:
        if _is_unexpected_keyword_error(exc, "text"):
            return generate(text, **kwargs)
        raise


def _supports_text_keyword(generate: Callable[..., Any]) -> bool:
    try:
        signature = inspect.signature(generate)
    except (TypeError, ValueError):
        return True
    parameters = signature.parameters
    return "text" in parameters or any(
        parameter.kind == inspect.Parameter.VAR_KEYWORD
        for parameter in parameters.values()
    )


def _normalize_speech_result(raw_result: Any, *, model: Any) -> dict[str, object]:
    results = _result_items(raw_result)
    if not results:
        raise ValueError("MLX audio speech runtime produced no audio chunks")

    sample_rate = _sample_rate(results, model=model)
    audio_chunks = [_audio_payload(item) for item in results]
    return {
        "audio": audio_chunks,
        "sample_rate": sample_rate,
    }


def _result_items(raw_result: Any) -> list[Any]:
    if _audio_payload_or_none(raw_result) is not None:
        return [raw_result]
    if isinstance(raw_result, Iterable) and not isinstance(
        raw_result,
        (bytes, bytearray, dict, str),
    ):
        return list(raw_result)
    return [raw_result]


def _sample_rate(results: list[Any], *, model: Any) -> int:
    for result in results:
        for key in ("sampling_rate", "sample_rate", "rate"):
            value = _value(result, key)
            if value is not None:
                return _positive_int(value, label=key)

    for key in ("sampling_rate", "sample_rate", "rate"):
        value = _value(model, key)
        if value is not None:
            return _positive_int(value, label=key)

    raise ValueError("MLX audio speech runtime result did not include a sample rate")


def _positive_int(value: object, *, label: str) -> int:
    sample_rate = int(value)
    if sample_rate <= 0:
        raise ValueError(
            f"MLX audio speech runtime returned invalid {label} {sample_rate}"
        )
    return sample_rate


def _audio_payload(item: Any) -> Any:
    payload = _audio_payload_or_none(item)
    if payload is None:
        raise ValueError(
            "MLX audio speech runtime result did not include an `audio`, "
            "`waveform`, or `array` payload"
        )
    return payload


def _audio_payload_or_none(item: Any) -> Any:
    for key in ("audio", "waveform", "array"):
        value = _value(item, key)
        if value is not None:
            return value
    return None


def _value(source: Any, key: str) -> object | None:
    if isinstance(source, dict):
        return source.get(key)
    return getattr(source, key, None)


def _is_unexpected_keyword_error(error: TypeError, keyword: str) -> bool:
    message = str(error).lower()
    return (
        f"unexpected keyword argument '{keyword}'" in message
        or f"unexpected keyword argument \"{keyword}\"" in message
        or f"got an unexpected keyword argument '{keyword}'" in message
        or f"got an unexpected keyword argument \"{keyword}\"" in message
    )
