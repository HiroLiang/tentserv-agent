from __future__ import annotations

from pathlib import Path
from typing import Any

from ..errors import missing_backend_dependency


def load_llama_class() -> Any:
    try:
        from llama_cpp import Llama
    except ModuleNotFoundError as exc:
        if exc.name == "llama_cpp":
            raise missing_backend_dependency(exc.name) from exc
        raise
    return Llama


def resolve_gguf_path(source_path: Path) -> Path:
    if source_path.is_file():
        if source_path.suffix.lower() == ".gguf":
            return source_path
        raise ValueError(f"expected a GGUF file, got `{source_path}`")

    matches = sorted(source_path.glob("*.gguf"))
    if not matches:
        raise FileNotFoundError(f"no GGUF file found under `{source_path}`")
    if len(matches) > 1:
        names = ", ".join(path.name for path in matches[:5])
        raise ValueError(
            "multiple GGUF files were found in the model source; "
            f"this runtime expects exactly one GGUF file (found: {names})"
        )
    return matches[0]
