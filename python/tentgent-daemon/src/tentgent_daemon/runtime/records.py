from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path


DEFAULT_QUALIFIER = "com"
DEFAULT_ORGANIZATION = "tentserv"
DEFAULT_APPLICATION = "com.tentserv.tentgent"
HOME_ENV = "TENTGENT_HOME"
MODELS_ENV = "TENTGENT_MODELS_DIR"


@dataclass(frozen=True)
class StoredModelRecord:
    model_ref: str
    short_ref: str
    source_kind: str
    source_repo: str | None
    source_revision: str | None
    source_path: str | None
    primary_format: str
    detected_formats: tuple[str, ...]
    file_count: int
    total_bytes: int
    imported_at: str
    store_path: Path
    manifest_path: Path
    variant_source_path: Path


def load_model_record(reference: str, home: Path | None = None) -> StoredModelRecord:
    store_dir = resolve_models_dir(home) / "store"
    if not store_dir.exists():
        raise FileNotFoundError("Tentgent model store does not exist yet.")

    exact_path = store_dir / reference / "model.toml"
    if exact_path.exists():
        return _read_record(exact_path.parent)

    matches: list[StoredModelRecord] = []
    for model_dir in sorted(store_dir.iterdir()):
        if not model_dir.is_dir():
            continue
        if model_dir.name.startswith(reference):
            matches.append(_read_record(model_dir))

    if not matches:
        raise LookupError(f"model reference `{reference}` was not found")

    if len(matches) > 1:
        raise LookupError(
            f"model reference `{reference}` is ambiguous; multiple stored models share that prefix"
        )

    return matches[0]


def resolve_models_dir(home: Path | None = None) -> Path:
    env_models = _read_env_path(MODELS_ENV)
    if env_models is not None:
        return env_models

    runtime_home = home or _read_env_path(HOME_ENV) or _default_home_dir()
    return runtime_home / "models"


def _read_record(model_dir: Path) -> StoredModelRecord:
    metadata_path = model_dir / "model.toml"
    with metadata_path.open("rb") as handle:
        raw = tomllib.load(handle)

    model_ref = _require_string(raw, "model_ref")
    primary_format = _require_string(raw, "primary_format")

    return StoredModelRecord(
        model_ref=model_ref,
        short_ref=_require_string(raw, "short_ref"),
        source_kind=_require_string(raw, "source_kind"),
        source_repo=_optional_string(raw, "source_repo"),
        source_revision=_optional_string(raw, "source_revision"),
        source_path=_optional_string(raw, "source_path"),
        primary_format=primary_format,
        detected_formats=tuple(raw.get("detected_formats", [])),
        file_count=int(raw.get("file_count", 0)),
        total_bytes=int(raw.get("total_bytes", 0)),
        imported_at=_require_string(raw, "imported_at"),
        store_path=model_dir,
        manifest_path=model_dir / "manifest.json",
        variant_source_path=model_dir / "variants" / primary_format / "source",
    )


def _require_string(raw: dict[str, object], key: str) -> str:
    value = raw.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"invalid or missing `{key}` in model metadata")
    return value


def _optional_string(raw: dict[str, object], key: str) -> str | None:
    value = raw.get(key)
    return value if isinstance(value, str) and value else None


def _read_env_path(name: str) -> Path | None:
    value = os.environ.get(name, "").strip()
    return Path(value).expanduser().resolve() if value else None


def _default_home_dir() -> Path:
    library = Path.home() / "Library" / "Application Support"
    return library / DEFAULT_APPLICATION
