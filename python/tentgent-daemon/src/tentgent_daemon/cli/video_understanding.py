from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_video_understanding_backend
from tentgent_daemon.runtime.video_understanding import (
    SUPPORTED_OUTPUT_FORMATS,
    VideoSamplingOptions,
    VideoUnderstandingRequest,
    build_video_understanding_plan,
    normalize_video_understanding_output_format,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent video-understanding request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument("--video-path", required=True, help="Video file path to inspect")
    parser.add_argument("--prompt", required=True, help="User prompt for the video")
    parser.add_argument("--system-prompt", help="Optional system prompt")
    parser.add_argument(
        "--format",
        required=True,
        choices=sorted(SUPPORTED_OUTPUT_FORMATS),
        help="Response output format intent",
    )
    parser.add_argument("--max-tokens", type=int, help="Optional max generated tokens")
    parser.add_argument("--temperature", type=float, help="Optional sampling temperature")
    parser.add_argument("--sample-fps", type=float, help="Optional video sample FPS")
    parser.add_argument("--max-frames", type=int, help="Optional sampled frame cap")
    parser.add_argument("--max-frame-edge", type=int, help="Optional sampled frame resize edge")
    parser.add_argument("--clip-start-seconds", type=float, help="Optional clip start offset")
    parser.add_argument("--clip-duration-seconds", type=float, help="Optional clip duration")
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running video inference.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_format = normalize_video_understanding_output_format(args.format)
    request = VideoUnderstandingRequest(
        model_ref=args.model_ref,
        video_path=Path(args.video_path),
        prompt=args.prompt,
        system_prompt=args.system_prompt,
        output_format=output_format,
        max_tokens=args.max_tokens,
        temperature=args.temperature,
        sampling=VideoSamplingOptions(
            sample_fps=args.sample_fps,
            max_frames=args.max_frames,
            max_frame_edge=args.max_frame_edge,
            clip_start_seconds=args.clip_start_seconds,
            clip_duration_seconds=args.clip_duration_seconds,
        ),
    )
    home = Path(args.home).expanduser().resolve() if args.home else None
    plan = build_video_understanding_plan(request, home=home)

    if args.plan_only:
        print(
            json.dumps(
                {
                    "model_ref": plan.record.model_ref,
                    "short_ref": plan.record.short_ref,
                    "backend": str(plan.backend),
                    "load_path": str(plan.load_path),
                    "video_path": str(plan.request.video_path),
                    "output_format": plan.request.output_format,
                    "sampling": {
                        "sample_fps": plan.request.sampling.sample_fps,
                        "max_frames": plan.request.sampling.max_frames,
                        "max_frame_edge": plan.request.sampling.max_frame_edge,
                        "clip_start_seconds": plan.request.sampling.clip_start_seconds,
                        "clip_duration_seconds": plan.request.sampling.clip_duration_seconds,
                    },
                },
                indent=2,
            )
        )
        return 0

    backend = create_video_understanding_backend(plan.backend)
    backend.load(plan.record)
    result = backend.understand_video(plan.request)
    print(
        json.dumps(
            {
                "model_ref": plan.record.model_ref,
                "output_format": result.output_format,
                "media_type": result.media_type,
                "text": result.text,
                "finish_reason": result.finish_reason,
                "sampled_frames": result.sampled_frames,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
