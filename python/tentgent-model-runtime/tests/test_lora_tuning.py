from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tentgent.runtime.backends.lora_tuning import (
    LoraBackendConfig,
    LoraCheckpointConfig,
    LoraConfig,
    LoraDatasetConfig,
    LoraOptimizationConfig,
    LoraTuningBackendKind,
    LoraTuningRequest,
    LoraTuningResult,
    normalize_lora_tuning_request,
)
from tentgent.runtime.backends.mlx.lora_tuning import (
    parse_mlx_line,
    write_mlx_config,
)
from tentgent.runtime.backends.records import ModelCapability, ModelFormat, ModelRecord
from tentgent.runtime.backends.transformers.lora_tuning import (
    TransformersPeftLoraTuningModel,
)
from tentgent.runtime.training.datasets import (
    IGNORE_INDEX,
    prepare_peft_datasets,
    render_training_dataset,
)


class LoraDatasetTests(unittest.TestCase):
    def test_render_training_dataset_and_masked_peft_labels(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            dataset_dir = Path(tmp) / "dataset"
            dataset_dir.mkdir()
            _write_train_split(dataset_dir)

            rendered = render_training_dataset(
                source_dir=dataset_dir,
                output_dir=Path(tmp) / "rendered",
                mask_prompt=True,
            )
            rendered_row = json.loads((rendered.output_dir / "train.jsonl").read_text())

            tokenized = prepare_peft_datasets(
                dataset_dir=dataset_dir,
                tokenizer=FakeTokenizer(),
                max_seq_length=256,
                mask_prompt=True,
            )

        self.assertEqual(rendered.splits[0].examples, 1)
        self.assertIn("User:", rendered_row["prompt"])
        self.assertEqual(rendered_row["completion"], "Hello back.")
        self.assertEqual(len(tokenized.train.examples), 1)
        self.assertIn(IGNORE_INDEX, tokenized.train.examples[0].labels)


class LoraRequestTests(unittest.TestCase):
    def test_normalize_rejects_non_chat_models(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            dataset_dir = Path(tmp) / "dataset"
            model_dir = Path(tmp) / "model"
            dataset_dir.mkdir()
            model_dir.mkdir()
            _write_train_split(dataset_dir)
            request = _lora_request(
                tmp,
                backend=LoraTuningBackendKind.PEFT,
                model=_model_record(
                    ModelFormat.SAFETENSORS,
                    source_path=model_dir,
                    capabilities=frozenset({ModelCapability.VIDEO_UNDERSTANDING}),
                ),
                dataset_dir=dataset_dir,
            )

            with self.assertRaises(ValueError) as raised:
                normalize_lora_tuning_request(request)

        self.assertIn("chat / causal-LM", str(raised.exception))


class PeftLoraTuningTests(unittest.TestCase):
    def test_peft_runner_prepares_dataset_and_delegates_training(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            dataset_dir = Path(tmp) / "dataset"
            model_dir = Path(tmp) / "model"
            dataset_dir.mkdir()
            model_dir.mkdir()
            _write_train_split(dataset_dir)
            request = normalize_lora_tuning_request(
                _lora_request(
                    tmp,
                    backend=LoraTuningBackendKind.PEFT,
                    model=_model_record(ModelFormat.SAFETENSORS, source_path=model_dir),
                    dataset_dir=dataset_dir,
                )
            )
            events: list[dict[str, object]] = []
            runner = TransformersPeftLoraTuningModel()
            runner.load(request.model)

            def fake_training(**kwargs: object) -> LoraTuningResult:
                adapter_path = kwargs["adapter_path"]
                assert isinstance(adapter_path, Path)
                adapter_path.mkdir(parents=True, exist_ok=True)
                adapter_file = adapter_path / "adapter_model.safetensors"
                adapter_file.write_bytes(b"adapter")
                return LoraTuningResult(
                    backend=LoraTuningBackendKind.PEFT,
                    model_ref=request.model.model_ref,
                    output_dir=request.output_dir,
                    adapter_path=adapter_path,
                    adapter_file=adapter_file,
                )

            with (
                patch(
                    "tentgent.runtime.backends.transformers.lora_tuning.load_peft_tokenizer",
                    return_value=FakeTokenizer(),
                ),
                patch(
                    "tentgent.runtime.backends.transformers.lora_tuning.run_peft_training",
                    side_effect=fake_training,
                ) as training,
            ):
                result = runner.run_lora_tuning(request, emit=events.append)
                adapter_exists = bool(result.adapter_file and result.adapter_file.exists())

        self.assertEqual(result.backend, LoraTuningBackendKind.PEFT)
        self.assertTrue(adapter_exists)
        self.assertGreater(training.call_args.kwargs["tokenized"].train.token_count, 0)
        self.assertEqual(events[0]["type"], "stage")
        self.assertTrue(any(event.get("type") == "dataset" for event in events))


class MlxLoraTuningTests(unittest.TestCase):
    def test_mlx_config_renders_dataset_and_lora_parameters(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            dataset_dir = Path(tmp) / "dataset"
            model_dir = Path(tmp) / "model"
            dataset_dir.mkdir()
            model_dir.mkdir()
            _write_train_split(dataset_dir)
            request = normalize_lora_tuning_request(
                _lora_request(
                    tmp,
                    backend=LoraTuningBackendKind.MLX,
                    model=_model_record(ModelFormat.MLX, source_path=model_dir),
                    dataset_dir=dataset_dir,
                    lora=LoraConfig(rank=4, scale=8.0, target_modules=("q_proj",)),
                )
            )
            events: list[dict[str, object]] = []
            config_path = write_mlx_config(
                request=request,
                run_dir=request.output_dir,
                emit=events.append,
            )
            config = json.loads(config_path.read_text())
            rendered_train_exists = (Path(config["data"]) / "train.jsonl").exists()

        self.assertEqual(config["model"], str(request.model.source_path))
        self.assertEqual(config["lora_parameters"]["rank"], 4)
        self.assertEqual(config["lora_parameters"]["keys"], ["q_proj"])
        self.assertTrue(rendered_train_exists)
        self.assertEqual(events[0]["type"], "dataset")

    def test_mlx_progress_parser_maps_training_events(self) -> None:
        request = _lora_request(
            "/tmp",
            backend=LoraTuningBackendKind.MLX,
            model=_model_record(ModelFormat.MLX),
            dataset_dir=Path("/tmp/dataset"),
        )
        events = parse_mlx_line(
            "Iter 2: Train loss 1.25, Learning Rate 1e-05, It/sec 0.5, "
            "Tokens/sec 10.0, Trained Tokens 20, Peak mem 1.2 GB",
            request=request,
        )

        self.assertEqual(events[0]["type"], "train")
        self.assertEqual(events[0]["step"], 2)
        self.assertEqual(events[0]["max_steps"], 2)


class LoraTuningRouteTests(unittest.TestCase):
    def test_route_payload_builds_normalized_request(self) -> None:
        from tentgent.runtime.server.routes.lora_tuning import (
            LoraDatasetPayload,
            LoraTuningPayload,
            _build_lora_tuning_request,
        )
        from tentgent.runtime.server.routes.payloads import ModelRecordPayload

        with tempfile.TemporaryDirectory() as tmp:
            dataset_dir = Path(tmp) / "dataset"
            model_dir = Path(tmp) / "model"
            dataset_dir.mkdir()
            model_dir.mkdir()
            _write_train_split(dataset_dir)
            payload = LoraTuningPayload(
                backend=LoraTuningBackendKind.PEFT,
                model=ModelRecordPayload(
                    model_ref="model-ref",
                    source_path=str(model_dir),
                    primary_format=ModelFormat.SAFETENSORS,
                    capabilities=[ModelCapability.CHAT],
                ),
                dataset=LoraDatasetPayload(source_path=str(dataset_dir)),
                output_dir=str(Path(tmp) / "run"),
            )
            task_ref, request = _build_lora_tuning_request(payload)

        self.assertTrue(task_ref)
        self.assertEqual(request.backend, LoraTuningBackendKind.PEFT)
        self.assertEqual(request.dataset.source_path, dataset_dir.resolve())


def _write_train_split(dataset_dir: Path) -> None:
    rows = [
        {
            "schema": "tentgent.chat.v1",
            "messages": [
                {"role": "system", "content": "Answer briefly."},
                {"role": "user", "content": "Say hello."},
                {"role": "assistant", "content": "Hello back."},
            ],
        }
    ]
    (dataset_dir / "train.jsonl").write_text(
        "\n".join(json.dumps(row) for row in rows) + "\n",
        encoding="utf-8",
    )


def _model_record(
    format_: ModelFormat,
    *,
    source_path: Path = Path("/tmp/model"),
    capabilities: frozenset[ModelCapability] = frozenset({ModelCapability.CHAT}),
) -> ModelRecord:
    return ModelRecord(
        model_ref="model-ref",
        source_path=source_path,
        primary_format=format_,
        capabilities=capabilities,
    )


def _lora_request(
    tmp: str | Path,
    *,
    backend: LoraTuningBackendKind,
    model: ModelRecord,
    dataset_dir: Path,
    lora: LoraConfig | None = None,
) -> LoraTuningRequest:
    return LoraTuningRequest(
        backend=backend,
        model=model,
        dataset=LoraDatasetConfig(
            source_path=dataset_dir,
            max_seq_length=256,
            mask_prompt=True,
        ),
        output_dir=Path(tmp) / "run",
        lora=lora or LoraConfig(rank=4),
        optimization=LoraOptimizationConfig(max_steps=2, batch_size=1),
        checkpoint=LoraCheckpointConfig(
            log_every_steps=1,
            eval_every_steps=2,
            save_every_steps=2,
        ),
        backend_config=LoraBackendConfig(),
    )


class FakeTokenizer:
    eos_token = "</s>"
    pad_token_id = 0
    eos_token_id = 1

    def __call__(self, text: str, **kwargs: object) -> dict[str, list[int]]:
        max_length = int(kwargs.get("max_length") or 4096)
        ids = [ord(char) % 97 + 2 for char in text]
        if kwargs.get("add_special_tokens"):
            ids.append(self.eos_token_id)
        return {"input_ids": ids[:max_length]}

    def save_pretrained(self, path: Path) -> None:
        (path / "tokenizer.json").write_text("{}", encoding="utf-8")


if __name__ == "__main__":
    unittest.main()
