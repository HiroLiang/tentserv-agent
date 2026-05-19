from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_audio_transcription_backend
from tentgent_daemon.runtime.audio import (
    SUPPORTED_OUTPUT_FORMATS,
    AudioTranscriptionRequest,
    build_audio_transcription_plan,
    normalize_audio_transcription_output_format,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent batch audio transcription request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument("--input-path", required=True, help="Audio file path to transcribe")
    parser.add_argument("--output-path", required=True, help="Transcript output path")
    parser.add_argument(
        "--format",
        required=True,
        choices=sorted(SUPPORTED_OUTPUT_FORMATS),
        help="Transcript output format",
    )
    parser.add_argument("--language", help="Optional model language hint, such as en")
    parser.add_argument(
        "--timestamps",
        action="store_true",
        help="Ask the runtime to return timestamp chunks when supported.",
    )
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running transcription inference.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_format = normalize_audio_transcription_output_format(args.format)
    request = AudioTranscriptionRequest(
        model_ref=args.model_ref,
        input_path=Path(args.input_path),
        output_path=Path(args.output_path),
        output_format=output_format,
        language=args.language,
        timestamps=args.timestamps,
    )
    home = Path(args.home).expanduser().resolve() if args.home else None
    plan = build_audio_transcription_plan(request, home=home)

    if args.plan_only:
        print(
            json.dumps(
                {
                    "model_ref": plan.record.model_ref,
                    "short_ref": plan.record.short_ref,
                    "backend": str(plan.backend),
                    "load_path": str(plan.load_path),
                    "input_path": str(plan.request.input_path),
                    "output_path": str(plan.request.output_path),
                    "output_format": plan.request.output_format,
                    "timestamps": plan.request.timestamps,
                },
                indent=2,
            )
        )
        return 0

    backend = create_audio_transcription_backend(plan.backend)
    backend.load(plan.record)
    result = backend.transcribe(plan.request)
    print(
        json.dumps(
            {
                "model_ref": plan.record.model_ref,
                "output_format": result.output_format,
                "media_type": result.media_type,
                "output_path": str(result.output_path),
                "total_bytes": result.total_bytes,
                "text": result.text,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
