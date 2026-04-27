"""LoRA training runner entry point."""

from __future__ import annotations

import argparse
import tomllib
from pathlib import Path
from typing import Any

from tentgent_daemon.runtime.capabilities import ensure_backend_supported
from tentgent_daemon.training.mlx_lora import run_mlx_lora
from tentgent_daemon.training.peft_lora import run_peft_lora
from tentgent_daemon.training.skeleton import run_skeleton


def main() -> int:
    args = parse_args()
    plan = load_plan(args.plan_file)
    backend = plan.get("backend") or "unknown"

    if backend == "mlx":
        ensure_backend_supported(backend)
        return run_mlx_lora(
            plan=plan,
            plan_ref=args.plan_ref,
            run_ref=args.run_ref,
            run_dir=args.run_dir,
        )
    if backend == "peft":
        return run_peft_lora(
            plan=plan,
            plan_ref=args.plan_ref,
            run_ref=args.run_ref,
            run_dir=args.run_dir,
        )
    return run_skeleton(
        plan=plan,
        plan_ref=args.plan_ref,
        run_ref=args.run_ref,
        run_dir=args.run_dir,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Emit Tentgent LoRA training events without running a backend trainer.",
    )
    parser.add_argument("--plan-ref", required=True)
    parser.add_argument("--plan-file", required=True, type=Path)
    parser.add_argument("--run-dir", required=True, type=Path)
    parser.add_argument("--run-ref", required=True)
    return parser.parse_args()


def load_plan(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


if __name__ == "__main__":
    raise SystemExit(main())
