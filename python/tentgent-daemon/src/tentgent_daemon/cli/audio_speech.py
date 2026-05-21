from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_audio_speech_backend
from tentgent_daemon.runtime.audio_speech import (
    SUPPORTED_OUTPUT_FORMATS,
    AudioSpeechRequest,
    build_audio_speech_plan,
    normalize_audio_speech_output_format,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent text-to-speech request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument("--text", required=True, help="Text to synthesize")
    parser.add_argument("--output-path", required=True, help="Speech audio output path")
    parser.add_argument(
        "--format",
        required=True,
        choices=sorted(SUPPORTED_OUTPUT_FORMATS),
        help="Speech output format",
    )
    parser.add_argument("--language", help="Optional model language hint, such as en")
    parser.add_argument("--voice", help="Optional model voice or speaker hint")
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running speech inference.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_format = normalize_audio_speech_output_format(args.format)
    request = AudioSpeechRequest(
        model_ref=args.model_ref,
        text=args.text,
        output_path=Path(args.output_path),
        output_format=output_format,
        language=args.language,
        voice=args.voice,
    )
    home = Path(args.home).expanduser().resolve() if args.home else None
    plan = build_audio_speech_plan(request, home=home)

    if args.plan_only:
        print(
            json.dumps(
                {
                    "model_ref": plan.record.model_ref,
                    "short_ref": plan.record.short_ref,
                    "backend": str(plan.backend),
                    "load_path": str(plan.load_path),
                    "output_path": str(plan.request.output_path),
                    "output_format": plan.request.output_format,
                    "language": plan.request.language,
                    "voice": plan.request.voice,
                },
                indent=2,
            )
        )
        return 0

    backend = create_audio_speech_backend(plan.backend)
    backend.load(plan.record)
    result = backend.synthesize_speech(plan.request)
    print(
        json.dumps(
            {
                "model_ref": plan.record.model_ref,
                "output_format": result.output_format,
                "media_type": result.media_type,
                "output_path": str(result.output_path),
                "total_bytes": result.total_bytes,
                "sample_rate": result.sample_rate,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
