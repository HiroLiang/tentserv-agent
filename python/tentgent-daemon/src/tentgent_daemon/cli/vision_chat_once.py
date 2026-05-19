from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_vision_chat_backend
from tentgent_daemon.runtime.vision import (
    SUPPORTED_OUTPUT_FORMATS,
    VisionChatRequest,
    build_vision_chat_plan,
    normalize_vision_chat_output_format,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent vision-chat request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument("--image-path", required=True, help="Image file path to inspect")
    parser.add_argument("--prompt", required=True, help="User prompt for the image")
    parser.add_argument("--system-prompt", help="Optional system prompt")
    parser.add_argument(
        "--format",
        required=True,
        choices=sorted(SUPPORTED_OUTPUT_FORMATS),
        help="Response output format intent",
    )
    parser.add_argument("--max-tokens", type=int, help="Optional max generated tokens")
    parser.add_argument("--temperature", type=float, help="Optional sampling temperature")
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running vision inference.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_format = normalize_vision_chat_output_format(args.format)
    request = VisionChatRequest(
        model_ref=args.model_ref,
        image_path=Path(args.image_path),
        prompt=args.prompt,
        system_prompt=args.system_prompt,
        output_format=output_format,
        max_tokens=args.max_tokens,
        temperature=args.temperature,
    )
    home = Path(args.home).expanduser().resolve() if args.home else None
    plan = build_vision_chat_plan(request, home=home)

    if args.plan_only:
        print(
            json.dumps(
                {
                    "model_ref": plan.record.model_ref,
                    "short_ref": plan.record.short_ref,
                    "backend": str(plan.backend),
                    "load_path": str(plan.load_path),
                    "image_path": str(plan.request.image_path),
                    "output_format": plan.request.output_format,
                },
                indent=2,
            )
        )
        return 0

    backend = create_vision_chat_backend(plan.backend)
    backend.load(plan.record)
    result = backend.generate_vision_chat(plan.request)
    print(
        json.dumps(
            {
                "model_ref": plan.record.model_ref,
                "output_format": result.output_format,
                "media_type": result.media_type,
                "text": result.text,
                "finish_reason": result.finish_reason,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
