"""Provider-backed dataset evaluation CLI."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Sequence

from tentgent_daemon.datasets.eval import evaluate_dataset
from tentgent_daemon.datasets.provider import DatasetProviderError


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        outcome = evaluate_dataset(
            provider=args.provider,
            model=args.model,
            dataset_path=args.input,
            output_dir=args.output,
            split=args.split,
            max_records=args.max_records,
            criteria=args.criteria,
            max_tokens=args.max_tokens,
            temperature=args.temperature,
            timeout_seconds=args.timeout_seconds,
            api_key=provider_api_key(args.provider),
        )
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "provider": outcome.provider,
                "model": outcome.model,
                "split": outcome.split,
                "input_path": str(outcome.input_path),
                "output_dir": str(outcome.output_dir),
                "report_json_path": str(outcome.report_json_path),
                "report_md_path": str(outcome.report_md_path),
                "prompt_path": str(outcome.prompt_path),
                "raw_output_path": str(outcome.raw_output_path),
                "reviewed_records": outcome.reviewed_records,
                "total_records": outcome.total_records,
                "local_issue_count": outcome.local_issue_count,
                "finding_count": outcome.finding_count,
                "overall_score": outcome.overall_score,
                "warnings": list(outcome.warnings),
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--provider", required=True, choices=("openai", "anthropic", "claude"))
    parser.add_argument("--model", required=True)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--split",
        default="train",
        choices=("train", "valid", "test", "eval_cases", "all"),
    )
    parser.add_argument("--max-records", type=int, default=20)
    parser.add_argument("--criteria")
    parser.add_argument("--max-tokens", type=int)
    parser.add_argument("--temperature", type=float, default=0.0)
    parser.add_argument("--timeout-seconds", type=float, default=180.0)
    return parser


def provider_api_key(provider: str) -> str:
    env_name = provider_env_var(provider)
    secret = os.environ.get(env_name, "").strip()
    if not secret:
        raise DatasetProviderError(f"missing provider API key in {env_name}")
    return secret


def provider_env_var(provider: str) -> str:
    match provider:
        case "openai":
            return "OPENAI_API_KEY"
        case "anthropic" | "claude":
            return "ANTHROPIC_API_KEY"
        case _:
            raise DatasetProviderError(f"unsupported provider `{provider}`")


if __name__ == "__main__":
    raise SystemExit(main())
