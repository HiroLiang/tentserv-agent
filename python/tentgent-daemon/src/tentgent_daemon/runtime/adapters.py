from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path

from .records import (
    DEFAULT_APPLICATION,
    HOME_ENV,
    StoredModelRecord,
)
from .router import BackendKind

ADAPTERS_ENV = "TENTGENT_ADAPTERS_DIR"


class AdapterNotFoundError(LookupError):
    pass


class AdapterAmbiguousError(LookupError):
    pass


class AdapterIncompatibleError(ValueError):
    pass


class AdapterBackendUnsupportedError(NotImplementedError):
    pass


class AdapterExecutionNotImplementedError(NotImplementedError):
    pass


@dataclass(frozen=True)
class StoredAdapterRecord:
    adapter_ref: str
    short_ref: str
    adapter_format: str
    adapter_type: str
    target_capability: str | None
    base_model_ref: str | None
    base_model_source_repo: str | None
    base_model_source_revision: str | None
    model_family: str | None
    backend_support: tuple[str, ...]
    weight_file: str | None
    trigger_words: tuple[str, ...]
    recommended_scale: float | None
    source_kind: str
    source_repo: str | None
    source_revision: str | None
    source_path: str | None
    file_count: int
    total_bytes: int
    imported_at: str
    store_path: Path
    manifest_path: Path
    source_dir: Path


def load_adapter_record(reference: str, home: Path | None = None) -> StoredAdapterRecord:
    store_dir = resolve_adapters_dir(home) / "store"
    if not store_dir.exists():
        raise AdapterNotFoundError("Tentgent adapter store does not exist yet.")

    exact_path = store_dir / reference / "adapter.toml"
    if exact_path.exists():
        return _read_record(exact_path.parent)

    matches: list[StoredAdapterRecord] = []
    for adapter_dir in sorted(store_dir.iterdir()):
        if not adapter_dir.is_dir():
            continue
        if adapter_dir.name.startswith(reference):
            matches.append(_read_record(adapter_dir))

    if not matches:
        raise AdapterNotFoundError(f"adapter reference `{reference}` was not found")

    if len(matches) > 1:
        raise AdapterAmbiguousError(
            f"adapter reference `{reference}` is ambiguous; multiple stored adapters share that prefix"
        )

    return matches[0]


def validate_adapter_for_model(
    adapter: StoredAdapterRecord,
    model: StoredModelRecord,
    backend: BackendKind,
) -> None:
    compatibility_proven = False

    if adapter.base_model_ref:
        if adapter.base_model_ref != model.model_ref:
            raise AdapterIncompatibleError(
                f"adapter `{adapter.short_ref}` targets model `{adapter.base_model_ref}`, "
                f"but server model is `{model.model_ref}`"
            )
        compatibility_proven = True

    if (
        adapter.base_model_source_repo
        and model.source_repo
        and adapter.base_model_source_repo != model.source_repo
    ):
        raise AdapterIncompatibleError(
            f"adapter `{adapter.short_ref}` targets `{adapter.base_model_source_repo}`, "
            f"but server model source is `{model.source_repo}`"
        )

    if (
        adapter.base_model_source_repo
        and model.source_repo
        and adapter.base_model_source_repo == model.source_repo
    ):
        compatibility_proven = True

    if (
        adapter.base_model_source_revision
        and model.source_revision
        and adapter.base_model_source_revision != model.source_revision
    ):
        raise AdapterIncompatibleError(
            f"adapter `{adapter.short_ref}` targets revision "
            f"`{adapter.base_model_source_revision}`, but server model revision is "
            f"`{model.source_revision}`"
        )

    if not compatibility_proven:
        raise AdapterIncompatibleError(
            f"adapter `{adapter.short_ref}` is not bound to this local model and "
            "does not declare a matching base model source"
        )

    support = _backend_support_name(backend)
    if support not in adapter.backend_support:
        raise AdapterBackendUnsupportedError(
            f"adapter `{adapter.short_ref}` supports {', '.join(adapter.backend_support) or 'no backends'}, "
            f"but server backend is `{support}`"
        )


def resolve_adapters_dir(home: Path | None = None) -> Path:
    env_adapters = _read_env_path(ADAPTERS_ENV)
    if env_adapters is not None:
        return env_adapters

    runtime_home = home or _read_env_path(HOME_ENV) or _default_home_dir()
    return runtime_home / "adapters"


def _read_record(adapter_dir: Path) -> StoredAdapterRecord:
    metadata_path = adapter_dir / "adapter.toml"
    with metadata_path.open("rb") as handle:
        raw = tomllib.load(handle)

    adapter_ref = _require_string(raw, "adapter_ref")

    return StoredAdapterRecord(
        adapter_ref=adapter_ref,
        short_ref=_require_string(raw, "short_ref"),
        adapter_format=_require_string(raw, "adapter_format"),
        adapter_type=_require_string(raw, "adapter_type"),
        target_capability=_optional_string(raw, "target_capability"),
        base_model_ref=_optional_string(raw, "base_model_ref"),
        base_model_source_repo=_optional_string(raw, "base_model_source_repo"),
        base_model_source_revision=_optional_string(raw, "base_model_source_revision"),
        model_family=_optional_string(raw, "model_family"),
        backend_support=tuple(
            item for item in raw.get("backend_support", []) if isinstance(item, str)
        ),
        weight_file=_optional_string(raw, "weight_file"),
        trigger_words=tuple(
            item for item in raw.get("trigger_words", []) if isinstance(item, str)
        ),
        recommended_scale=_optional_float(raw, "recommended_scale"),
        source_kind=_require_string(raw, "source_kind"),
        source_repo=_optional_string(raw, "source_repo"),
        source_revision=_optional_string(raw, "source_revision"),
        source_path=_optional_string(raw, "source_path"),
        file_count=int(raw.get("file_count", 0)),
        total_bytes=int(raw.get("total_bytes", 0)),
        imported_at=_require_string(raw, "imported_at"),
        store_path=adapter_dir,
        manifest_path=adapter_dir / "manifest.json",
        source_dir=adapter_dir / "source",
    )


def _backend_support_name(backend: BackendKind) -> str:
    if backend == BackendKind.MLX:
        return "mlx"
    if backend == BackendKind.MLX_DIFFUSION:
        return "mlx-diffusion"
    if backend == BackendKind.DIFFUSERS:
        return "diffusers"
    if backend == BackendKind.TRANSFORMERS_PEFT:
        return "transformers-peft"
    if backend == BackendKind.LLAMA_CPP:
        return "llama-cpp"
    return str(backend)


def _require_string(raw: dict[str, object], key: str) -> str:
    value = raw.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"invalid or missing `{key}` in adapter metadata")
    return value


def _optional_string(raw: dict[str, object], key: str) -> str | None:
    value = raw.get(key)
    return value if isinstance(value, str) and value else None


def _optional_float(raw: dict[str, object], key: str) -> float | None:
    value = raw.get(key)
    if isinstance(value, (int, float)):
        return float(value)
    return None


def _read_env_path(name: str) -> Path | None:
    value = os.environ.get(name, "").strip()
    return Path(value).expanduser().resolve() if value else None


def _default_home_dir() -> Path:
    library = Path.home() / "Library" / "Application Support"
    return library / DEFAULT_APPLICATION
