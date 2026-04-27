"""MLX LoRA runner that translates backend logs to Tentgent events."""

from __future__ import annotations

import json
import os
import queue
import re
import subprocess
import sys
import threading
from pathlib import Path
from typing import Any

from tentgent_daemon.datasets.render import render_training_dataset

from .events import emit

PARAMS_RE = re.compile(
    r"Trainable parameters:\s+(?P<percent>[0-9.]+)%\s+"
    r"\((?P<trainable>[0-9.]+)M/(?P<total>[0-9.]+)M\)"
)
TRAIN_RE = re.compile(
    r"Iter (?P<step>\d+): Train loss (?P<loss>[0-9.]+), "
    r"Learning Rate (?P<lr>[0-9.eE+-]+), It/sec (?P<it_sec>[0-9.]+), "
    r"Tokens/sec (?P<tokens_sec>[0-9.]+), Trained Tokens (?P<trained>\d+), "
    r"Peak mem (?P<memory>[0-9.]+) GB"
)
EVAL_RE = re.compile(
    r"Iter (?P<step>\d+): Val loss (?P<loss>[0-9.]+), "
    r"Val took (?P<duration>[0-9.]+)s"
)
CHECKPOINT_RE = re.compile(
    r"Iter (?P<step>\d+): Saved adapter weights to (?P<adapter>.+?) and (?P<checkpoint>.+)\.$"
)
FINAL_RE = re.compile(r"Saved final weights to (?P<path>.+)\.$")
START_RE = re.compile(r"Starting training\.\.\., iters: (?P<iters>\d+)")


def run_mlx_lora(
    *,
    plan: dict[str, Any],
    plan_ref: str,
    run_ref: str,
    run_dir: Path,
) -> int:
    config_path = write_mlx_config(plan=plan, run_dir=run_dir)
    adapter_path = run_dir / "adapter-output"
    command = [sys.executable, "-m", "mlx_lm", "lora", "--config", str(config_path)]

    emit({"type": "stage", "name": "launch_mlx", "status": "started"})
    process = subprocess.Popen(
        command,
        cwd=run_dir,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
        env={**os.environ, "PYTHONUNBUFFERED": "1"},
    )

    lines: queue.Queue[tuple[str, str | None]] = queue.Queue()
    threads = [
        threading.Thread(target=read_stream, args=("stdout", process.stdout, lines)),
        threading.Thread(target=read_stream, args=("stderr", process.stderr, lines)),
    ]
    for thread in threads:
        thread.daemon = True
        thread.start()

    live_streams = len(threads)
    while live_streams:
        stream, line = lines.get()
        if line is None:
            live_streams -= 1
            continue
        mirror_raw(stream, line)
        for event in parse_mlx_line(line, plan=plan):
            emit(event)

    status = process.wait()
    for thread in threads:
        thread.join()

    if status == 0:
        emit(
            {
                "type": "done",
                "run_ref": run_ref,
                "plan_ref": plan_ref,
                "adapter_path": str(adapter_path),
                "adapter_file": str(adapter_path / "adapters.safetensors"),
            }
        )
    else:
        emit(
            {
                "type": "error",
                "backend": "mlx",
                "exit_code": status,
                "message": "mlx_lm.lora exited with a non-zero status",
            }
        )
    return status


def write_mlx_config(*, plan: dict[str, Any], run_dir: Path) -> Path:
    adapter_path = run_dir / "adapter-output"
    adapter_path.mkdir(parents=True, exist_ok=True)

    optimization = plan.get("optimization", {})
    checkpoint = plan.get("checkpoint", {})
    dataset = plan.get("dataset", {})
    lora = plan.get("lora", {})
    mlx = plan.get("backend_config", {}).get("mlx", {})
    rendered_dataset = render_training_dataset(
        source_dir=Path(dataset.get("source_path")),
        output_dir=run_dir / "rendered-data",
        mask_prompt=bool(dataset.get("mask_prompt", False)),
    )
    emit_rendered_dataset_summary(rendered_dataset)

    config = {
        "model": plan.get("model", {}).get("source_path"),
        "train": True,
        "test": False,
        "data": str(rendered_dataset.output_dir),
        "fine_tune_type": mlx.get("fine_tune_type", "lora"),
        "optimizer": optimization.get("optimizer", "adamw"),
        "seed": optimization.get("seed", 0),
        "num_layers": mlx.get("num_layers", 16),
        "batch_size": optimization.get("batch_size", 1),
        "iters": optimization.get("max_steps", 100),
        "val_batches": mlx.get("val_batches", 25),
        "learning_rate": optimization.get("learning_rate", 1e-5),
        "steps_per_report": checkpoint.get("log_every_steps", 10),
        "steps_per_eval": checkpoint.get("eval_every_steps", 200),
        "grad_accumulation_steps": optimization.get("gradient_accumulation_steps", 1),
        "adapter_path": str(adapter_path),
        "save_every": checkpoint.get("save_every_steps", 100),
        "test_batches": mlx.get("test_batches", 500),
        "max_seq_length": dataset.get("max_seq_length", 2048),
        "grad_checkpoint": mlx.get("grad_checkpoint", False),
        "mask_prompt": dataset.get("mask_prompt", False),
        "lora_parameters": lora_parameters(lora),
    }
    config_path = run_dir / "mlx-config.yaml"
    config_path.write_text(json.dumps(config, indent=2, sort_keys=True), encoding="utf-8")
    return config_path


def emit_rendered_dataset_summary(rendered_dataset: Any) -> None:
    split_counts = {split.name: split.examples for split in rendered_dataset.splits}
    emit(
        {
            "type": "dataset",
            "backend": "mlx",
            "train_examples": split_counts.get("train", 0),
            "validation_examples": split_counts.get("valid", 0),
            "test_examples": split_counts.get("test", 0),
            "eval_cases": rendered_dataset.eval_cases,
            "rendered_path": str(rendered_dataset.output_dir),
        }
    )


def lora_parameters(lora: dict[str, Any]) -> dict[str, Any]:
    params: dict[str, Any] = {
        "rank": lora.get("rank", 8),
        "dropout": lora.get("dropout", 0.0),
        "scale": lora.get("scale", 20.0),
    }
    if keys := lora.get("target_modules"):
        params["keys"] = keys
    return params


def read_stream(
    stream_name: str,
    stream: Any,
    lines: queue.Queue[tuple[str, str | None]],
) -> None:
    if stream is None:
        lines.put((stream_name, None))
        return
    for raw in stream:
        for line in split_progress_line(raw):
            lines.put((stream_name, line))
    lines.put((stream_name, None))


def split_progress_line(raw: str) -> list[str]:
    return [part.strip() for part in raw.replace("\r", "\n").splitlines() if part.strip()]


def mirror_raw(stream_name: str, line: str) -> None:
    print(f"[mlx:{stream_name}] {line}", file=sys.stderr, flush=True)


def parse_mlx_line(line: str, *, plan: dict[str, Any]) -> list[dict[str, Any]]:
    if line == "Loading pretrained model":
        return [{"type": "stage", "name": "load_model", "status": "completed"}]
    if line == "Loading datasets":
        return [{"type": "stage", "name": "load_dataset", "status": "completed"}]
    if match := START_RE.search(line):
        return [
            {
                "type": "stage",
                "name": "train",
                "status": "started",
                "max_steps": int(match["iters"]),
            }
        ]
    if match := PARAMS_RE.search(line):
        return [params_event(match)]
    if match := TRAIN_RE.search(line):
        return [train_event(match, plan)]
    if match := EVAL_RE.search(line):
        return [eval_event(match)]
    if match := CHECKPOINT_RE.search(line):
        return [checkpoint_event(match)]
    if match := FINAL_RE.search(line):
        return [
            {
                "type": "checkpoint",
                "step": int(plan.get("optimization", {}).get("max_steps") or 0),
                "path": match["path"],
                "final": True,
            }
        ]
    return []


def params_event(match: re.Match[str]) -> dict[str, Any]:
    return {
        "type": "params",
        "trainable": round(float(match["trainable"]) * 1_000_000),
        "total": round(float(match["total"]) * 1_000_000),
        "percent": float(match["percent"]),
        "backend": "mlx",
    }


def train_event(match: re.Match[str], plan: dict[str, Any]) -> dict[str, Any]:
    return {
        "type": "train",
        "step": int(match["step"]),
        "max_steps": int(plan.get("optimization", {}).get("max_steps") or match["step"]),
        "loss": float(match["loss"]),
        "learning_rate": float(match["lr"]),
        "iterations_per_sec": float(match["it_sec"]),
        "tokens_per_sec": float(match["tokens_sec"]),
        "trained_tokens": int(match["trained"]),
        "peak_memory_gb": float(match["memory"]),
    }


def eval_event(match: re.Match[str]) -> dict[str, Any]:
    return {
        "type": "eval",
        "step": int(match["step"]),
        "loss": float(match["loss"]),
        "duration_sec": float(match["duration"]),
    }


def checkpoint_event(match: re.Match[str]) -> dict[str, Any]:
    return {
        "type": "checkpoint",
        "step": int(match["step"]),
        "path": match["checkpoint"],
        "adapter_file": match["adapter"],
    }
