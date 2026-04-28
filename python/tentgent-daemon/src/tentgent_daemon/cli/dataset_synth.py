"""Provider-backed file-first dataset synthesis CLI."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Sequence

from tentgent_daemon.datasets.provider import (
    DatasetJsonlGenerationRequest,
    DatasetProviderError,
    DatasetSplitKind,
    generate_dataset_jsonl,
)
from tentgent_daemon.datasets.synth import (
    DATASET_TEMPLATE_VERSION,
    build_dataset_generation_prompt,
    prompt_source,
    write_dataset_synth_package,
)


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        source_kind, source_text = resolve_prompt_source(args)
        prompt = build_dataset_generation_prompt(
            brief=source_text if source_kind == "brief" else None,
            spec=source_text if source_kind == "spec" else None,
            split=args.split,
        )
        response = generate_dataset_jsonl(
            DatasetJsonlGenerationRequest(
                provider=args.provider,
                model=args.model,
                prompt=prompt,
                split=args.split,
                max_tokens=args.max_tokens,
                temperature=args.temperature,
            ),
            api_key=provider_api_key(args.provider),
        )
        outcome = write_dataset_synth_package(
            output_dir=args.output,
            provider=response.provider,
            model=response.model,
            split=response.split,
            jsonl=response.jsonl,
            record_count=len(response.records),
            prompt_source_kind=source_kind,
            prompt_source_text=source_text,
            prompt_source_path=str(args.spec) if source_kind == "spec" else None,
            warnings=response.warnings,
            max_tokens=args.max_tokens,
            temperature=args.temperature,
        )
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1

    print(
        json.dumps(
            {
                "provider": response.provider,
                "model": response.model,
                "split": response.split,
                "output_dir": str(outcome.output_dir),
                "split_path": str(outcome.split_path),
                "manifest_path": str(outcome.manifest_path),
                "record_count": outcome.record_count,
                "template_version": DATASET_TEMPLATE_VERSION,
                "warnings": list(response.warnings),
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
    parser.add_argument("--output", required=True, type=Path)
    input_group = parser.add_mutually_exclusive_group(required=True)
    input_group.add_argument("--brief")
    input_group.add_argument("--spec", type=Path)
    parser.add_argument(
        "--split",
        default="train",
        choices=("train", "valid", "test", "eval_cases"),
    )
    parser.add_argument("--max-tokens", type=int)
    parser.add_argument("--temperature", type=float, default=0.0)
    return parser


def resolve_prompt_source(args: argparse.Namespace) -> tuple[str, str]:
    spec_text = None
    if args.spec is not None:
        spec_text = args.spec.read_text(encoding="utf-8")
    return prompt_source(brief=args.brief, spec=spec_text)


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
