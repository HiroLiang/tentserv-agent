from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import pytest
from tentgent.runtime.backends.diffusers.image_generation import (
    DiffusersImageGenerationModel,
)
from tentgent.runtime.backends.image_generation import ImageGenerationWorkflowKind
from tentgent.runtime.backends.mlx.chat import MlxChatModel
from tentgent.runtime.backends.records import ModelFormat, ModelRecord
from tentgent.runtime.backends.transformers.audio_speech import (
    TransformersAudioSpeechModel,
)
from tentgent.runtime.backends.transformers.audio_transcription import (
    TransformersAudioTranscriptionModel,
)
from tentgent.runtime.backends.transformers.base import (
    load_transformers_component,
    load_transformers_model,
)
from tentgent.runtime.backends.transformers.lora_tuning import (
    load_peft_tokenizer,
    run_peft_training,
)


def test_components_never_opt_into_repository_code() -> None:
    component = Mock()
    assert (
        load_transformers_component(component, "model")
        is component.from_pretrained.return_value
    )
    component.from_pretrained.assert_called_once_with("model", trust_remote_code=False)


def test_models_require_safe_weights_and_builtin_code() -> None:
    component = Mock()
    model = load_transformers_model(component, "model", "cpu")
    component.from_pretrained.assert_called_once_with(
        "model", trust_remote_code=False, use_safetensors=True
    )
    model.to.assert_called_once_with("cpu")
    model.eval.assert_called_once_with()


@pytest.mark.parametrize(
    "model_class", [TransformersAudioSpeechModel, TransformersAudioTranscriptionModel]
)
def test_audio_pipelines_do_not_trust_repository_code(model_class: type) -> None:
    model = object.__new__(model_class)
    torch = Mock()
    torch.cuda.is_available.return_value = False
    torch.backends.mps.is_available.return_value = False
    torch.device.return_value = SimpleNamespace(type="cpu")
    pipeline = Mock()
    model._deps = SimpleNamespace(torch=torch, pipeline=pipeline)
    record = ModelRecord(
        model_ref="test-model",
        source_path=Path("model"),
        primary_format=ModelFormat.SAFETENSORS,
    )
    model.load(record)
    assert pipeline.call_args.kwargs["trust_remote_code"] is False
    assert pipeline.call_args.kwargs["model_kwargs"] == {"use_safetensors": True}


def test_training_tokenizer_does_not_trust_repository_code() -> None:
    tokenizer = Mock(pad_token_id=1)
    factory = Mock()
    factory.from_pretrained.return_value = tokenizer
    with patch.dict(
        sys.modules, {"transformers": SimpleNamespace(AutoTokenizer=factory)}
    ):
        assert load_peft_tokenizer(Path("model"), emit=lambda _: None) is tokenizer
    factory.from_pretrained.assert_called_once_with("model", trust_remote_code=False)


def test_training_model_does_not_trust_repository_code() -> None:
    factory = Mock()
    factory.from_pretrained.side_effect = RuntimeError("stop-before-training")
    request = SimpleNamespace(
        backend_config=SimpleNamespace(peft={}),
        optimization=SimpleNamespace(seed=0),
    )
    with (
        patch.dict(
            sys.modules,
            {
                "transformers": SimpleNamespace(AutoModelForCausalLM=factory),
                "torch": Mock(),
                "peft": SimpleNamespace(get_peft_model=Mock()),
            },
        ),
        patch(
            "tentgent.runtime.backends.transformers.lora_tuning.detect_device",
            return_value="cpu",
        ),
        patch(
            "tentgent.runtime.backends.transformers.lora_tuning.torch_dtype",
            return_value="auto",
        ),
        pytest.raises(RuntimeError, match="stop-before-training"),
    ):
        run_peft_training(
            request=request,
            tokenizer=Mock(),
            tokenized=Mock(),
            model_path=Path("model"),
            adapter_path=Path("adapter"),
            emit=lambda _: None,
        )
    assert factory.from_pretrained.call_args.kwargs["trust_remote_code"] is False
    assert factory.from_pretrained.call_args.kwargs["use_safetensors"] is True


def test_mlx_tokenizer_has_explicit_no_remote_code_policy() -> None:
    model = MlxChatModel()
    model._load_path = "model"
    loader = Mock(return_value=(Mock(), Mock()))
    with patch(
        "tentgent.runtime.backends.mlx.chat._load_mlx_symbols",
        return_value=(loader, None, None, None),
    ):
        model._load_model(adapter_path=None)
    assert loader.call_args.kwargs["tokenizer_config"] == {"trust_remote_code": False}


@pytest.mark.parametrize("weight_name", ["../outside.safetensors", "weights.bin"])
def test_image_adapter_rejects_unsafe_weights_before_loading(
    tmp_path: Path, weight_name: str
) -> None:
    source = tmp_path / "adapter"
    source.mkdir()
    (tmp_path / "outside.safetensors").touch()
    (source / "weights.bin").touch()
    model = object.__new__(DiffusersImageGenerationModel)
    model._record = Mock()
    model._pipeline = Mock()
    model._adapter = SimpleNamespace(
        source_path=source, weight_file=weight_name, lora_scale=1.0
    )
    with pytest.raises(ValueError):
        model._apply_selected_adapter()
    model._pipeline.load_lora_weights.assert_not_called()


def test_image_adapter_requires_safetensors_at_loader(tmp_path: Path) -> None:
    (tmp_path / "weights.safetensors").touch()
    model = object.__new__(DiffusersImageGenerationModel)
    model._record = Mock()
    model._pipeline = Mock()
    model._adapter = SimpleNamespace(
        source_path=tmp_path, weight_file="weights.safetensors", lora_scale=1.0
    )
    model._apply_selected_adapter()
    assert model._pipeline.load_lora_weights.call_args.kwargs["use_safetensors"] is True


def test_invalid_image_adapter_never_leaves_a_reusable_base_pipeline(
    tmp_path: Path,
) -> None:
    (tmp_path / "weights.bin").touch()
    model = object.__new__(DiffusersImageGenerationModel)
    model._record = ModelRecord(
        model_ref="image", source_path=tmp_path, primary_format=ModelFormat.DIFFUSERS
    )
    model._pipeline = None
    model._pipeline_workflow = None
    model._device = SimpleNamespace(type="cpu")
    model._deps = Mock()
    model._adapter = SimpleNamespace(
        source_path=tmp_path, weight_file="weights.bin", lora_scale=1.0
    )
    for _ in range(2):
        with pytest.raises(ValueError, match="safetensors"):
            model._load_pipeline(
                ImageGenerationWorkflowKind.TEXT_TO_IMAGE, SimpleNamespace()
            )
        assert model._pipeline is None
        assert model._pipeline_workflow is None
    assert model._deps.DiffusionPipeline.from_pretrained.call_count == 2
    model._deps.DiffusionPipeline.from_pretrained.return_value.load_lora_weights.assert_not_called()


def test_controlnet_assets_are_validated_before_native_loading(tmp_path: Path) -> None:
    (tmp_path / "config.json").write_text(
        json.dumps({"auto_map": {"AutoModel": "custom.Model"}})
    )
    model = object.__new__(DiffusersImageGenerationModel)
    model._record = SimpleNamespace(source_path=tmp_path)
    model._pipeline = None
    model._device = SimpleNamespace(type="cpu")
    model._deps = Mock()
    request = SimpleNamespace(
        control=SimpleNamespace(
            source_path=tmp_path, control_ref="control", control_kind="canny"
        )
    )
    with pytest.raises(ValueError, match="auto_map"):
        model._load_pipeline(ImageGenerationWorkflowKind.CONTROL, request)
    model._deps.ControlNetModel.from_pretrained.assert_not_called()


def test_real_transformers_rejects_custom_config_without_executing_it(
    tmp_path: Path,
) -> None:
    transformers = pytest.importorskip("transformers")
    marker = tmp_path / "executed"
    (tmp_path / "config.json").write_text(
        json.dumps(
            {
                "model_type": "tentgent_test_custom",
                "auto_map": {"AutoConfig": "configuration_fixture.CustomConfig"},
            }
        )
    )
    (tmp_path / "configuration_fixture.py").write_text(
        "from pathlib import Path\n"
        f"Path({str(marker)!r}).touch()\n"
        "from transformers import PretrainedConfig\n"
        "class CustomConfig(PretrainedConfig):\n"
        "    model_type = 'tentgent_test_custom'\n"
    )
    with pytest.raises(ValueError, match="trust_remote_code"):
        load_transformers_component(transformers.AutoConfig, str(tmp_path))
    assert not marker.exists()
