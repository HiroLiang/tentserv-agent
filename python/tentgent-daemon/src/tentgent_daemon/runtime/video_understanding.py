from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .records import StoredModelRecord, load_model_record
from .router import BackendKind, resolve_video_understanding_backend


TEXT_FORMAT = "text"
JSON_FORMAT = "json"
MD_FORMAT = "md"
SUPPORTED_OUTPUT_FORMATS = {TEXT_FORMAT, JSON_FORMAT, MD_FORMAT}


@dataclass(frozen=True)
class VideoSamplingOptions:
    sample_fps: float | None = 1.0
    max_frames: int | None = 32
    max_frame_edge: int | None = 768
    clip_start_seconds: float | None = None
    clip_duration_seconds: float | None = None


@dataclass(frozen=True)
class VideoUnderstandingRequest:
    model_ref: str
    video_path: Path
    prompt: str
    output_format: str = TEXT_FORMAT
    system_prompt: str | None = None
    max_tokens: int | None = None
    temperature: float | None = None
    sampling: VideoSamplingOptions = VideoSamplingOptions()


@dataclass(frozen=True)
class VideoUnderstandingResult:
    output_format: str
    media_type: str
    text: str
    finish_reason: str = "stop"
    sampled_frames: int | None = None


@dataclass(frozen=True)
class VideoUnderstandingPlan:
    request: VideoUnderstandingRequest
    record: StoredModelRecord
    backend: BackendKind
    load_path: Path


def build_video_understanding_plan(
    request: VideoUnderstandingRequest,
    home: Path | None = None,
) -> VideoUnderstandingPlan:
    record = load_model_record(request.model_ref, home=home)
    if "video-understanding" not in record.model_capabilities:
        capabilities = ", ".join(record.model_capabilities)
        raise ValueError(
            "video understanding endpoint requires model capability "
            f"`video-understanding`, but model `{record.model_ref}` advertises "
            f"[{capabilities}]"
        )

    video_path = request.video_path.expanduser().resolve()
    if not video_path.is_file():
        raise FileNotFoundError(
            f"video understanding input video `{video_path}` was not found"
        )

    prompt = request.prompt.strip()
    if not prompt:
        raise ValueError("video understanding prompt must not be empty")

    system_prompt = request.system_prompt.strip() if request.system_prompt else None
    if system_prompt == "":
        system_prompt = None

    output_format = normalize_video_understanding_output_format(request.output_format)
    sampling = validate_video_sampling_options(request.sampling)
    return VideoUnderstandingPlan(
        request=VideoUnderstandingRequest(
            model_ref=request.model_ref,
            video_path=video_path,
            prompt=prompt,
            system_prompt=system_prompt,
            output_format=output_format,
            max_tokens=request.max_tokens,
            temperature=request.temperature,
            sampling=sampling,
        ),
        record=record,
        backend=resolve_video_understanding_backend(record),
        load_path=record.variant_source_path,
    )


def normalize_video_understanding_output_format(value: str) -> str:
    normalized = value.strip().lower()
    if normalized in {"txt", ""}:
        normalized = TEXT_FORMAT
    if normalized == "markdown":
        normalized = MD_FORMAT
    if normalized not in SUPPORTED_OUTPUT_FORMATS:
        expected = ", ".join(sorted(SUPPORTED_OUTPUT_FORMATS))
        raise ValueError(
            f"unsupported video understanding output format `{value}`; "
            f"expected one of: {expected}"
        )
    return normalized


def video_understanding_media_type(output_format: str) -> str:
    output_format = normalize_video_understanding_output_format(output_format)
    if output_format == JSON_FORMAT:
        return "application/json"
    if output_format == MD_FORMAT:
        return "text/markdown"
    return "text/plain"


def validate_video_sampling_options(
    value: VideoSamplingOptions,
) -> VideoSamplingOptions:
    sample_fps = value.sample_fps if value.sample_fps is not None else 1.0
    max_frames = value.max_frames if value.max_frames is not None else 32
    max_frame_edge = value.max_frame_edge if value.max_frame_edge is not None else 768

    if not 0.1 <= sample_fps <= 4.0:
        raise ValueError(f"`sample_fps` must be between 0.1 and 4.0; got {sample_fps}")
    if not 1 <= max_frames <= 128:
        raise ValueError(f"`max_frames` must be between 1 and 128; got {max_frames}")
    if not 128 <= max_frame_edge <= 1536:
        raise ValueError(
            f"`max_frame_edge` must be between 128 and 1536; got {max_frame_edge}"
        )
    if value.clip_start_seconds is not None and value.clip_start_seconds < 0:
        raise ValueError(
            "`clip_start_seconds` must be greater than or equal to 0; "
            f"got {value.clip_start_seconds}"
        )
    if value.clip_duration_seconds is not None and value.clip_duration_seconds <= 0:
        raise ValueError(
            "`clip_duration_seconds` must be greater than 0; "
            f"got {value.clip_duration_seconds}"
        )

    return VideoSamplingOptions(
        sample_fps=sample_fps,
        max_frames=max_frames,
        max_frame_edge=max_frame_edge,
        clip_start_seconds=value.clip_start_seconds,
        clip_duration_seconds=value.clip_duration_seconds,
    )
