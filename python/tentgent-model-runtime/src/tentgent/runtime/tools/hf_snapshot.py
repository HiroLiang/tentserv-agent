from __future__ import annotations

import argparse
import itertools
import json
import os
import shutil
import sys
import threading
from collections.abc import Iterable
from pathlib import Path
from typing import Any

os.environ.setdefault("HF_HUB_DISABLE_PROGRESS_BARS", "1")

from huggingface_hub import HfApi, snapshot_download


class JsonProgressTqdm:
    _lock = threading.RLock()
    _ids = itertools.count(1)

    def __init__(
        self,
        iterable: Iterable[Any] | None = None,
        desc: str | None = None,
        total: int | float | None = None,
        initial: int | float = 0,
        unit: str = "it",
        **_: Any,
    ) -> None:
        self.iterable = iterable
        self.desc = desc or ""
        self.total = total
        self.n = initial
        self.unit = unit
        self.progress_id = next(self._ids)
        self.closed = False
        self._emit("start")

    @classmethod
    def get_lock(cls) -> threading.RLock:
        return cls._lock

    @classmethod
    def set_lock(cls, lock: threading.RLock) -> None:
        cls._lock = lock

    def __iter__(self):
        try:
            for item in self.iterable or ():
                yield item
                self.update(1)
        finally:
            self.close()

    def __enter__(self) -> "JsonProgressTqdm":
        return self

    def __exit__(self, _exc_type: object, _exc_value: object, _traceback: object) -> None:
        self.close()

    def update(self, n: int | float | None = 1) -> None:
        if n is not None:
            self.n += n
        self._emit("update")

    def refresh(self, *_: Any, **__: Any) -> None:
        self._emit("refresh")

    def set_description(self, desc: str | None = None, refresh: bool = True) -> None:
        self.desc = desc or ""
        if refresh:
            self._emit("description")

    def close(self) -> None:
        if self.closed:
            return
        self.closed = True
        self._emit("close")

    def _emit(self, kind: str) -> None:
        payload = {
            "event": "progress",
            "kind": kind,
            "id": self.progress_id,
            "desc": self.desc,
            "position": self.n,
            "total": self.total,
            "unit": self.unit,
        }
        with self._lock:
            print(json.dumps(payload, separators=(",", ":")), flush=True)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Resolve and download a Hugging Face snapshot for Tentgent."
    )
    parser.add_argument("--repo-id", required=True)
    parser.add_argument("--revision")
    parser.add_argument("--local-dir", required=True)
    parser.add_argument("--result-path", required=True)
    parser.add_argument("--progress-json", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
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
            tqdm_class=JsonProgressTqdm if args.progress_json else None,
        )
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1

    cache_dir = local_dir / ".cache"
    if cache_dir.exists():
        shutil.rmtree(cache_dir)

    write_result_file(
        result_path,
        repo_id=args.repo_id,
        resolved_revision=resolved_revision,
        local_dir=local_dir,
        metadata=build_hf_metadata(info, local_dir),
    )
    return 0


def build_hf_metadata(info: Any, local_dir: Path) -> dict[str, Any]:
    return {
        "pipeline_tag": _optional_string(getattr(info, "pipeline_tag", None)),
        "tags": _string_list(getattr(info, "tags", None)),
        "library_name": _optional_string(getattr(info, "library_name", None)),
        "config_architectures": _config_architectures(local_dir / "config.json"),
        "tokenizer_chat_template": _tokenizer_has_chat_template(
            local_dir / "tokenizer_config.json"
        ),
        "sentence_bert_config": (local_dir / "sentence_bert_config.json").is_file(),
    }


def write_result_file(
    result_path: Path,
    *,
    repo_id: str,
    resolved_revision: str,
    local_dir: Path,
    metadata: dict[str, Any],
) -> None:
    result_path.write_text(
        json.dumps(
            {
                "repo_id": repo_id,
                "resolved_revision": resolved_revision,
                "local_dir": str(local_dir),
                "metadata": metadata,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def _optional_string(value: Any) -> str | None:
    if isinstance(value, str) and value.strip():
        return value
    return None


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, (list, tuple, set)):
        return []
    return [item for item in value if isinstance(item, str) and item.strip()]


def _config_architectures(path: Path) -> list[str]:
    body = _read_json_object(path)
    architectures = body.get("architectures")
    if isinstance(architectures, str) and architectures.strip():
        return [architectures]
    return _string_list(architectures)


def _tokenizer_has_chat_template(path: Path) -> bool:
    body = _read_json_object(path)
    template = body.get("chat_template")
    return isinstance(template, str) and bool(template.strip())


def _read_json_object(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    try:
        parsed = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return parsed if isinstance(parsed, dict) else {}


if __name__ == "__main__":
    raise SystemExit(main())
