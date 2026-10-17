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
            if _is_projector(source_path):
                raise ValueError(
                    f"expected a primary GGUF model, got projector `{source_path}`"
                )
            return source_path
        raise ValueError(f"expected a GGUF file, got `{source_path}`")

    matches = sorted(
        path
        for path in source_path.glob("*")
        if path.is_file()
        and path.suffix.lower() == ".gguf"
        and not _is_projector(path)
    )
    if not matches:
        raise FileNotFoundError(
            f"no primary GGUF model found under `{source_path}`; "
            "mmproj projector files cannot be loaded as standalone models"
        )
    if len(matches) > 1:
        names = ", ".join(path.name for path in matches[:5])
        raise ValueError(
            "multiple primary GGUF files were found in the model source; "
            f"this runtime expects exactly one primary GGUF file (found: {names})"
        )
    return matches[0]


def _is_projector(path: Path) -> bool:
    # Keep this filename convention aligned with kernel file diagnostics.
    stem = path.stem.lower()
    return stem == "mmproj" or stem.startswith(("mmproj-", "mmproj_"))
