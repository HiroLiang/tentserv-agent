from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_rerank_backend
from tentgent_daemon.runtime.rerank import RerankRequest, build_rerank_plan


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent one-shot rerank request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument("--query", required=True, help="Query text for reranking")
    parser.add_argument(
        "--document",
        action="append",
        dest="documents",
        required=True,
        help="Candidate document text. Repeat to preserve original document indexes.",
    )
    parser.add_argument("--top-n", type=int, help="Optional number of ranked results to return")
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running rerank inference.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    documents = tuple(_validate_document(value) for value in args.documents)
    top_n = _validate_top_n(args.top_n, len(documents))
    request = RerankRequest(
        model_ref=args.model_ref,
        query=_validate_query(args.query),
        documents=documents,
        top_n=top_n,
    )
    home = Path(args.home).expanduser().resolve() if args.home else None
    plan = build_rerank_plan(request, home=home)

    if args.plan_only:
        print(
            json.dumps(
                {
                    "model_ref": plan.record.model_ref,
                    "short_ref": plan.record.short_ref,
                    "backend": str(plan.backend),
                    "load_path": str(plan.load_path),
                    "document_count": len(plan.request.documents),
                    "top_n": plan.request.top_n,
                },
                indent=2,
            )
        )
        return 0

    backend = create_rerank_backend(plan.backend)
    backend.load(plan.record)
    result = backend.rerank(request)
    print(
        json.dumps(
            {
                "model_ref": plan.record.model_ref,
                "data": [
                    {"index": item.index, "score": item.score}
                    for item in result.data
                ],
            },
            separators=(",", ":"),
        )
    )
    return 0


def _validate_query(value: str) -> str:
    if not value.strip():
        raise ValueError("rerank query must not be empty")
    return value


def _validate_document(value: str) -> str:
    if not value.strip():
        raise ValueError("rerank documents must not be empty")
    return value


def _validate_top_n(value: int | None, document_count: int) -> int | None:
    if value is None:
        return None
    if value < 1 or value > document_count:
        raise ValueError("rerank top_n must be between 1 and document count")
    return value


if __name__ == "__main__":
    raise SystemExit(main())
