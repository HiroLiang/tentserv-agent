from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_embedding_backend
from tentgent_daemon.runtime.embedding import EmbeddingRequest, build_embedding_plan


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent one-shot embedding request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument(
        "--input",
        action="append",
        dest="inputs",
        required=True,
        help="Input text to embed. Repeat to preserve request order.",
    )
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running embedding inference.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    request = EmbeddingRequest(
        model_ref=args.model_ref,
        inputs=tuple(_validate_input(value) for value in args.inputs),
    )
    home = Path(args.home).expanduser().resolve() if args.home else None
    plan = build_embedding_plan(request, home=home)

    if args.plan_only:
        print(
            json.dumps(
                {
                    "model_ref": plan.record.model_ref,
                    "short_ref": plan.record.short_ref,
                    "backend": str(plan.backend),
                    "load_path": str(plan.load_path),
                    "input_count": len(plan.request.inputs),
                },
                indent=2,
            )
        )
        return 0

    backend = create_embedding_backend(plan.backend)
    backend.load(plan.record)
    result = backend.embed(request)
    print(
        json.dumps(
            {
                "model_ref": plan.record.model_ref,
                "data": [
                    {"index": index, "embedding": vector}
                    for index, vector in enumerate(result.vectors)
                ],
            },
            separators=(",", ":"),
        )
    )
    return 0


def _validate_input(value: str) -> str:
    if not value.strip():
        raise ValueError("embedding input strings must not be empty")
    return value


if __name__ == "__main__":
    raise SystemExit(main())
