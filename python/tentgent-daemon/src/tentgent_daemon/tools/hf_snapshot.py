#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
from pathlib import Path

from huggingface_hub import HfApi, snapshot_download


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Resolve a Hugging Face model revision and download a full snapshot "
            "into a Tentgent staging directory."
        )
    )
    parser.add_argument("--repo-id", required=True, help="Hugging Face repo id")
    parser.add_argument(
        "--revision",
        help="Optional branch, tag, or commit to resolve before downloading",
    )
    parser.add_argument(
        "--local-dir",
        required=True,
        help="Local staging directory where the snapshot should be materialized",
    )
    parser.add_argument(
        "--result-path",
        required=True,
        help="JSON output path for the resolved pull result",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    token = os.environ.get("HF_TOKEN") or None
    local_dir = Path(args.local_dir).expanduser().resolve()
    result_path = Path(args.result_path).expanduser().resolve()
    local_dir.mkdir(parents=True, exist_ok=True)
    result_path.parent.mkdir(parents=True, exist_ok=True)

    try:
        api = HfApi(token=token)
        info = api.model_info(args.repo_id, revision=args.revision, token=token)
        resolved_revision = getattr(info, "sha", None)
        if not resolved_revision:
            raise RuntimeError(
                f"Hugging Face did not return a resolved commit SHA for {args.repo_id}."
            )

        snapshot_download(
            repo_id=args.repo_id,
            revision=resolved_revision,
            token=token,
            local_dir=str(local_dir),
        )
    except Exception as exc:  # pragma: no cover - CLI integration handles the surface area.
        print(str(exc), file=sys.stderr)
        return 1

    cache_dir = local_dir / ".cache"
    if cache_dir.exists():
        shutil.rmtree(cache_dir)

    result_path.write_text(
        json.dumps(
            {
                "repo_id": args.repo_id,
                "resolved_revision": resolved_revision,
                "local_dir": str(local_dir),
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
