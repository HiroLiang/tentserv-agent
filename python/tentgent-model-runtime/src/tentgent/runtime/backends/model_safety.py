"""Reject executable model metadata and unsafe shard paths before native loading.

Managed model stores contain materialized local assets, not Hub-cache symlinks.
This is a fail-closed preflight, not a sandbox against a concurrent local OS user
who can replace trusted files after inspection or modify installed libraries.
"""

from __future__ import annotations

import importlib.util
import json
import os
import stat
from pathlib import Path, PureWindowsPath
from typing import Any

from .records import ModelFormat, ModelRecord

MAX_MODEL_CONFIG_BYTES = 8 * 1024 * 1024


class ModelAssetSafetyError(ValueError):
    """A managed model requires unsupported code or unsafe local asset access."""


def validate_model_assets(record: ModelRecord) -> None:
    validate_local_model_assets(record.source_path, record.primary_format)


def validate_local_model_assets(source: Path, primary_format: ModelFormat) -> None:
    """Validate local base-model, ControlNet or selected adapter assets."""
    try:
        source_mode = source.lstat().st_mode
        if stat.S_ISLNK(source_mode):
            raise ModelAssetSafetyError("managed model source must not be a symlink")
        if stat.S_ISREG(source_mode):
            if primary_format == ModelFormat.GGUF or source.suffix == ".safetensors":
                return
            raise ModelAssetSafetyError(
                "managed model weights must be safetensors or GGUF"
            )
        if not stat.S_ISDIR(source_mode):
            raise ModelAssetSafetyError(
                "managed model source must be a local directory"
            )
        root = source.resolve(strict=True)
        files = _regular_asset_files(root)
        for path in files:
            if primary_format == ModelFormat.DIFFUSERS and path.suffix in {
                ".py",
                ".pyc",
                ".pyo",
            }:
                raise ModelAssetSafetyError(
                    f"repository Python modules are not supported in managed Diffusers models: {path.name}"
                )
            if _is_loader_config(path):
                config = _read_config(path)
                _reject_auto_map(config, path)
                if path.name == "model_index.json":
                    _validate_diffusers_index(config, path)
                if (
                    path.name.endswith(".index.json")
                    and isinstance(config, dict)
                    and "weight_map" in config
                ):
                    _validate_shards(config["weight_map"], path, root)
    except ModelAssetSafetyError:
        raise
    except (OSError, ValueError, RecursionError) as error:
        raise ModelAssetSafetyError(
            f"cannot validate managed model assets: {error}"
        ) from error


def _regular_asset_files(root: Path) -> list[Path]:
    files = []

    def walk_error(error: OSError) -> None:
        raise error

    for directory, names, filenames in os.walk(
        root, followlinks=False, onerror=walk_error
    ):
        for name in [*names, *filenames]:
            path = Path(directory) / name
            mode = path.lstat().st_mode
            if stat.S_ISLNK(mode):
                target = path.resolve(strict=True)
                if not target.is_relative_to(root):
                    raise ModelAssetSafetyError(
                        f"model asset symlink escapes its source: {path}"
                    )
                mode = target.lstat().st_mode
                if not stat.S_ISREG(mode):
                    raise ModelAssetSafetyError(
                        f"model asset symlink must target a regular file: {path}"
                    )
            if stat.S_ISDIR(mode):
                continue
            if not stat.S_ISREG(mode):
                raise ModelAssetSafetyError(
                    f"model asset is not a regular file: {path}"
                )
            files.append(path)
    return files


def _is_loader_config(path: Path) -> bool:
    return (
        path.name in {"config.json", "model_index.json"}
        or (path.name.startswith("config.") and path.name.endswith(".json"))
        or path.name.endswith("_config.json")
        or path.name.endswith(".index.json")
    )


def _read_config(path: Path) -> Any:
    if path.stat().st_size > MAX_MODEL_CONFIG_BYTES:
        raise ModelAssetSafetyError(f"model configuration exceeds size limit: {path}")
    with path.open("rb") as stream:
        content = stream.read(MAX_MODEL_CONFIG_BYTES + 1)
    if len(content) > MAX_MODEL_CONFIG_BYTES:
        raise ModelAssetSafetyError(f"model configuration exceeds size limit: {path}")
    return json.loads(content)


def _reject_auto_map(value: Any, path: Path) -> None:
    if isinstance(value, dict):
        if value.get("auto_map"):
            raise ModelAssetSafetyError(
                f"custom auto_map repository code is not supported for managed models: {path}"
            )
        for child in value.values():
            _reject_auto_map(child, path)
    elif isinstance(value, list):
        for child in value:
            _reject_auto_map(child, path)


def _validate_diffusers_index(config: Any, path: Path) -> None:
    if not isinstance(config, dict):
        raise ModelAssetSafetyError(f"invalid Diffusers model index: {path}")
    if "_class_name" in config and not isinstance(config["_class_name"], str):
        raise ModelAssetSafetyError(
            f"custom Diffusers pipeline code is not supported: {path}"
        )
    libraries = {"diffusers", "transformers"}
    # Short component libraries such as stable_diffusion refer to bundled
    # Diffusers pipeline modules. Discover names without importing native code.
    spec = importlib.util.find_spec("diffusers")
    if spec is not None and spec.origin:
        pipelines = Path(spec.origin).parent / "pipelines"
        if pipelines.is_dir():
            libraries.update(item.name for item in pipelines.iterdir() if item.is_dir())
    for name, component in config.items():
        if name.startswith("_") or not isinstance(component, list):
            continue
        if len(component) != 2:
            raise ModelAssetSafetyError(f"invalid Diffusers component metadata: {name}")
        library, component_class = component
        if library is None and component_class is None:
            continue
        if (
            not isinstance(library, str)
            or library not in libraries
            or not isinstance(component_class, str)
        ):
            raise ModelAssetSafetyError(
                f"custom Diffusers component code is not supported: {name}"
            )
        _relative_asset_path(name, path.parent)


def _relative_asset_path(value: Any, parent: Path) -> Path:
    if not isinstance(value, str) or not value or "\\" in value:
        raise ModelAssetSafetyError(
            "model asset reference must be a nonempty relative path"
        )
    relative = Path(value)
    if relative.is_absolute() or PureWindowsPath(value).drive or ".." in relative.parts:
        raise ModelAssetSafetyError(
            f"model asset reference escapes its source: {value}"
        )
    return parent / relative


def _validate_shards(weight_map: Any, index: Path, root: Path) -> None:
    if not isinstance(weight_map, dict):
        raise ModelAssetSafetyError(f"invalid checkpoint weight_map: {index}")
    for value in weight_map.values():
        path = _relative_asset_path(value, index.parent)
        resolved = path.resolve(strict=True)
        if not resolved.is_relative_to(root):
            raise ModelAssetSafetyError(
                f"checkpoint shard escapes model source: {value}"
            )
        if not stat.S_ISREG(resolved.lstat().st_mode):
            raise ModelAssetSafetyError(
                f"checkpoint shard is not a regular file: {value}"
            )
        if ".safetensors." in index.name and path.suffix != ".safetensors":
            raise ModelAssetSafetyError(
                f"safetensors index references a non-safetensors shard: {value}"
            )
