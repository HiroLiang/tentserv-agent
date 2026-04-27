"""Fallback runner used before a backend is wired in."""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

from .events import emit


def run_skeleton(
    *,
    plan: dict[str, Any],
    plan_ref: str,
    run_ref: str,
    run_dir: Path,
) -> int:
    backend = plan.get("backend") or "unknown"
    max_steps = int(plan.get("optimization", {}).get("max_steps") or 1)
    demo_steps = min(max_steps, 3)
    adapter_path = run_dir / "adapter-output"
    adapter_path.mkdir(parents=True, exist_ok=True)

    emit({"type": "stage", "name": "load_model", "status": "completed"})
    emit({"type": "stage", "name": "load_dataset", "status": "completed"})
    emit(
        {
            "type": "params",
            "trainable": 0,
            "total": int(plan.get("model", {}).get("total_bytes") or 0),
            "percent": 0.0,
            "backend": backend,
            "skeleton": True,
        }
    )

    for step in range(1, demo_steps + 1):
        emit(
            {
                "type": "train",
                "step": step,
                "max_steps": max_steps,
                "loss": round(4.0 - (step * 0.25), 3),
                "learning_rate": plan.get("optimization", {}).get("learning_rate", 0.0),
                "tokens_per_sec": 0.0,
                "peak_memory_gb": 0.0,
                "skeleton": True,
            }
        )

    emit({"type": "eval", "step": demo_steps, "loss": 3.123, "skeleton": True})
    emit(
        {
            "type": "checkpoint",
            "step": demo_steps,
            "path": str(run_dir / "checkpoint-skeleton"),
            "skeleton": True,
        }
    )
    emit(
        {
            "type": "done",
            "run_ref": run_ref,
            "plan_ref": plan_ref,
            "adapter_path": str(adapter_path),
            "skeleton": True,
        }
    )
    print(
        f"skeleton LoRA runner completed for backend `{backend}`; "
        "backend training is not wired yet",
        file=sys.stderr,
        flush=True,
    )
    return 0
