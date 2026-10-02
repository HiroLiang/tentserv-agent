from __future__ import annotations

import json
import os
from pathlib import Path, PureWindowsPath
from types import SimpleNamespace
from unittest.mock import Mock, patch

import pytest
from tentgent.runtime.backends.mlx.lora_tuning import MlxLoraTuningModel
from tentgent.runtime.backends.model_safety import (
    ModelAssetSafetyError,
    _relative_asset_path,
    validate_local_model_assets,
    validate_model_assets,
)
from tentgent.runtime.backends.records import ModelFormat, ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager


def record(source: Path, format_: ModelFormat = ModelFormat.MLX) -> ModelRecord:
    return ModelRecord(model_ref="fixture", source_path=source, primary_format=format_)


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def symlink(path: Path, target: Path) -> None:
    try:
        path.symlink_to(target, target_is_directory=target.is_dir())
    except (OSError, NotImplementedError):
        pytest.skip("symlinks are not available")


@pytest.mark.parametrize(
    "filename",
    [
        "config.json",
        "config.4.0.0.json",
        "tokenizer_config.json",
        "preprocessor_config.json",
        "processor_config.json",
    ],
)
def test_repository_auto_map_is_rejected_before_loading(tmp_path, filename):
    write_json(
        tmp_path / filename, {"nested": {"auto_map": {"AutoTokenizer": "custom.Model"}}}
    )
    backend = Mock(is_loaded=False)
    backend.release.side_effect = lambda: setattr(backend, "is_loaded", False)
    manager = ResourceManager(model_factory=lambda _: backend)
    with (
        pytest.raises(ModelAssetSafetyError, match="auto_map"),
        manager.lease_model("chat", record(tmp_path)),
    ):
        pytest.fail("unsafe repository code must not load")
    backend.load.assert_not_called()
    assert manager.snapshot()["model_resource_count"] == 0


def test_standard_configs_and_nested_safetensors_indexes_pass(tmp_path):
    write_json(tmp_path / "config.json", {"model_type": "qwen2", "auto_map": None})
    folder = tmp_path / "text_encoder"
    folder.mkdir()
    (folder / "model-00001.safetensors").write_bytes(b"fixture")
    write_json(
        folder / "model.safetensors.index.json",
        {"weight_map": {"weight": "model-00001.safetensors"}},
    )
    write_json(tmp_path / "catalog.index.json", {"entries": []})
    validate_model_assets(record(tmp_path))


def test_versioned_configuration_cannot_hide_custom_repository_code(tmp_path):
    write_json(tmp_path / "config.json", {"configuration_files": ["config.4.0.0.json"]})
    write_json(
        tmp_path / "config.4.0.0.json",
        {"auto_map": {"AutoConfig": "custom.Configuration"}},
    )
    with pytest.raises(ModelAssetSafetyError, match="auto_map"):
        validate_model_assets(record(tmp_path))


@pytest.mark.parametrize(
    "reference",
    [
        "../outside.safetensors",
        "/tmp/outside.safetensors",
        "C:/outside.safetensors",
        r"C:\outside.safetensors",
        r"..\outside.safetensors",
    ],
)
def test_shard_traversal_and_absolute_paths_are_rejected(tmp_path, reference):
    write_json(
        tmp_path / "model.safetensors.index.json", {"weight_map": {"weight": reference}}
    )
    with pytest.raises(ModelAssetSafetyError, match="relative path|escapes"):
        validate_model_assets(record(tmp_path))


@pytest.mark.parametrize(
    "reference",
    ["/rooted.safetensors", "C:drive-relative.safetensors", "C:/absolute.safetensors"],
)
def test_windows_rooted_and_drive_paths_are_rejected_before_join(reference):
    # Exercise Windows joining semantics on every host, not only Windows CI.
    with (
        patch("tentgent.runtime.backends.model_safety.Path", PureWindowsPath),
        pytest.raises(ModelAssetSafetyError, match="escapes"),
    ):
        _relative_asset_path(reference, PureWindowsPath("C:/managed/model"))


def test_windows_relative_shard_path_stays_under_selected_source():
    with patch("tentgent.runtime.backends.model_safety.Path", PureWindowsPath):
        source = PureWindowsPath("C:/managed/model")
        result = _relative_asset_path("shards/weights.safetensors", source)
        assert result == source / "shards/weights.safetensors"
        assert result.is_relative_to(source)


@pytest.mark.parametrize(
    "weight_map", [[], {"weight": None}, {"weight": 12}, {"weight": ""}]
)
def test_malformed_shard_references_are_rejected(tmp_path, weight_map):
    write_json(tmp_path / "model.safetensors.index.json", {"weight_map": weight_map})
    with pytest.raises(ModelAssetSafetyError):
        validate_model_assets(record(tmp_path))


def test_safetensors_index_cannot_redirect_to_pickle_weights(tmp_path):
    (tmp_path / "model.bin").write_bytes(b"not executed")
    write_json(
        tmp_path / "model.safetensors.index.json",
        {"weight_map": {"weight": "model.bin"}},
    )
    with pytest.raises(ModelAssetSafetyError, match="non-safetensors"):
        validate_model_assets(record(tmp_path))


@pytest.mark.parametrize("name", ["config.json", "shard.safetensors"])
def test_external_symlinks_are_rejected_before_json_reads(tmp_path, name):
    root = tmp_path / "model"
    root.mkdir()
    outside = tmp_path / "outside"
    outside.write_bytes(b"not a config")
    symlink(root / name, outside)
    with patch("tentgent.runtime.backends.model_safety._read_config") as read:
        with pytest.raises(ModelAssetSafetyError, match="symlink escapes"):
            validate_model_assets(record(root))
        read.assert_not_called()


def test_internal_regular_file_symlink_is_supported(tmp_path):
    (tmp_path / "weights.safetensors").write_bytes(b"fixture")
    symlink(tmp_path / "shard.safetensors", tmp_path / "weights.safetensors")
    write_json(
        tmp_path / "model.safetensors.index.json",
        {"weight_map": {"weight": "shard.safetensors"}},
    )
    validate_model_assets(record(tmp_path))


def test_directory_symlink_is_rejected_to_prevent_cycles(tmp_path):
    symlink(tmp_path / "loop", tmp_path)
    with pytest.raises(ModelAssetSafetyError, match="regular file"):
        validate_model_assets(record(tmp_path))


@pytest.mark.skipif(not hasattr(os, "mkfifo"), reason="FIFO requires POSIX")
@pytest.mark.parametrize("name", ["config.json", "shard.safetensors"])
def test_fifo_is_rejected_without_opening_it(tmp_path, name):
    os.mkfifo(tmp_path / name)
    with patch("tentgent.runtime.backends.model_safety._read_config") as read:
        with pytest.raises(ModelAssetSafetyError, match="not a regular file"):
            validate_model_assets(record(tmp_path))
        read.assert_not_called()


def test_configuration_read_is_bounded(tmp_path):
    (tmp_path / "config.json").write_bytes(b" " * 17)
    with (
        patch("tentgent.runtime.backends.model_safety.MAX_MODEL_CONFIG_BYTES", 16),
        pytest.raises(ModelAssetSafetyError, match="size limit"),
    ):
        validate_model_assets(record(tmp_path))


def test_malformed_configuration_fails_closed(tmp_path):
    (tmp_path / "config.json").write_text("not json")
    with pytest.raises(ModelAssetSafetyError, match="cannot validate"):
        validate_model_assets(record(tmp_path))


def test_missing_model_source_fails_closed(tmp_path):
    with pytest.raises(ModelAssetSafetyError, match="cannot validate"):
        validate_model_assets(record(tmp_path / "missing"))


@pytest.mark.parametrize("name", ["None.py", "component/custom.py", "compiled.pyc"])
def test_diffusers_local_python_modules_are_rejected(tmp_path, name):
    path = tmp_path / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"not executed")
    with pytest.raises(ModelAssetSafetyError, match="Python modules"):
        validate_model_assets(record(tmp_path, ModelFormat.DIFFUSERS))


def test_diffusers_dynamic_pipeline_metadata_is_rejected(tmp_path):
    write_json(
        tmp_path / "model_index.json", {"_class_name": ["custom", "CustomPipeline"]}
    )
    with pytest.raises(ModelAssetSafetyError, match="custom Diffusers pipeline"):
        validate_model_assets(record(tmp_path, ModelFormat.DIFFUSERS))


def test_diffusers_custom_component_library_is_rejected(tmp_path):
    write_json(
        tmp_path / "model_index.json",
        {"_class_name": "FluxPipeline", "transformer": ["arbitrary_package", "Model"]},
    )
    with pytest.raises(ModelAssetSafetyError, match="custom Diffusers component"):
        validate_model_assets(record(tmp_path, ModelFormat.DIFFUSERS))


def test_diffusers_builtin_components_are_supported(tmp_path):
    write_json(
        tmp_path / "model_index.json",
        {
            "_class_name": "FluxPipeline",
            "transformer": ["diffusers", "FluxTransformer2DModel"],
            "tokenizer": ["transformers", "T5Tokenizer"],
            "safety_checker": [None, None],
        },
    )
    validate_model_assets(record(tmp_path, ModelFormat.DIFFUSERS))


def test_direct_safe_weight_file_is_supported_but_pickle_is_not(tmp_path):
    safe = tmp_path / "adapter.safetensors"
    safe.write_bytes(b"fixture")
    validate_local_model_assets(safe, ModelFormat.SAFETENSORS)
    unsafe = tmp_path / "adapter.bin"
    unsafe.write_bytes(b"not executed")
    with pytest.raises(ModelAssetSafetyError, match="safetensors or GGUF"):
        validate_local_model_assets(unsafe, ModelFormat.SAFETENSORS)


def test_mlx_training_rechecks_assets_before_subprocess(tmp_path):
    model = record(tmp_path)
    runner = MlxLoraTuningModel()
    runner.load(model)
    write_json(
        tmp_path / "tokenizer_config.json",
        {"auto_map": {"AutoTokenizer": "custom.Tokenizer"}},
    )
    with patch("tentgent.runtime.backends.mlx.lora_tuning.run_mlx_command") as launch:
        with pytest.raises(ModelAssetSafetyError, match="auto_map"):
            runner.run_lora_tuning(SimpleNamespace(model=model), emit=lambda _: None)
        launch.assert_not_called()
