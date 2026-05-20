from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_image_generation_backend
from tentgent_daemon.runtime.image_generation import (
    DEFAULT_GUIDANCE_SCALE,
    DEFAULT_HEIGHT,
    DEFAULT_STEPS,
    DEFAULT_WIDTH,
    SUPPORTED_OUTPUT_FORMATS,
    ImageGenerationRequest,
    build_image_generation_plan,
    normalize_image_generation_output_format,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent text-to-image request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument("--prompt", required=True, help="Text prompt for image generation")
    parser.add_argument("--negative-prompt", help="Optional negative prompt")
    parser.add_argument("--output-path", required=True, help="Generated image output path")
    parser.add_argument(
        "--format",
        required=True,
        choices=sorted(SUPPORTED_OUTPUT_FORMATS),
        help="Image output format",
    )
    parser.add_argument("--width", type=int, default=DEFAULT_WIDTH, help="Output width")
    parser.add_argument("--height", type=int, default=DEFAULT_HEIGHT, help="Output height")
    parser.add_argument("--steps", type=int, default=DEFAULT_STEPS, help="Inference steps")
    parser.add_argument(
        "--guidance-scale",
        type=float,
        default=DEFAULT_GUIDANCE_SCALE,
        help="Classifier-free guidance scale",
    )
    parser.add_argument("--seed", type=int, help="Optional deterministic seed")
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running image generation.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_format = normalize_image_generation_output_format(args.format)
    request = ImageGenerationRequest(
        model_ref=args.model_ref,
        prompt=args.prompt,
        negative_prompt=args.negative_prompt,
        output_path=Path(args.output_path),
        output_format=output_format,
        width=args.width,
        height=args.height,
        steps=args.steps,
        guidance_scale=args.guidance_scale,
        seed=args.seed,
    )
    home = Path(args.home).expanduser().resolve() if args.home else None
    plan = build_image_generation_plan(request, home=home)

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
                    "width": plan.request.width,
                    "height": plan.request.height,
                    "steps": plan.request.steps,
                    "guidance_scale": plan.request.guidance_scale,
                    "seed": plan.request.seed,
                },
                indent=2,
            )
        )
        return 0

    backend = create_image_generation_backend(plan.backend)
    backend.load(plan.record)
    result = backend.generate_image(plan.request)
    print(
        json.dumps(
            {
                "model_ref": plan.record.model_ref,
                "output_format": result.output_format,
                "media_type": result.media_type,
                "output_path": str(result.output_path),
                "total_bytes": result.total_bytes,
                "width": result.width,
                "height": result.height,
                "seed": result.seed,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
