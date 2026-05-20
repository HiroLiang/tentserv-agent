from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tentgent_daemon.backends import create_vision_chat_backend
from tentgent_daemon.backends.mlx_vlm import MlxVlmDeps, MlxVlmVisionChatBackend
from tentgent_daemon.runtime.records import StoredModelRecord
from tentgent_daemon.runtime.router import BackendKind
from tentgent_daemon.runtime.vision import (
    VisionChatRequest,
    build_vision_chat_plan,
    normalize_vision_chat_output_format,
    vision_chat_media_type,
)


class VisionChatRuntimeTests(unittest.TestCase):
    def test_build_plan_accepts_vision_safetensors_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "c" * 64
            write_model_record(home, model_ref, ["vision-chat"])
            image_path = home / "fixtures" / "image.png"
            image_path.parent.mkdir(parents=True)
            image_path.write_bytes(b"image")

            plan = build_vision_chat_plan(
                VisionChatRequest(
                    model_ref=model_ref[:12],
                    image_path=image_path,
                    prompt=" describe ",
                    system_prompt=" be brief ",
                    output_format="markdown",
                    max_tokens=32,
                    temperature=0.0,
                ),
                home=home,
            )

            self.assertEqual(plan.record.model_ref, model_ref)
            self.assertEqual(plan.request.output_format, "md")
            self.assertEqual(plan.request.prompt, "describe")
            self.assertEqual(plan.request.system_prompt, "be brief")
            self.assertEqual(plan.request.image_path, image_path.resolve())
            self.assertEqual(plan.load_path.name, "source")

    def test_build_plan_rejects_non_vision_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "d" * 64
            write_model_record(home, model_ref, ["chat"])
            image_path = home / "image.png"
            image_path.write_bytes(b"image")

            with self.assertRaisesRegex(ValueError, "vision-chat"):
                build_vision_chat_plan(
                    VisionChatRequest(
                        model_ref=model_ref,
                        image_path=image_path,
                        prompt="describe",
                        output_format="text",
                    ),
                    home=home,
                )

    def test_output_format_helpers_validate_values(self) -> None:
        self.assertEqual(normalize_vision_chat_output_format("txt"), "text")
        self.assertEqual(normalize_vision_chat_output_format("markdown"), "md")
        self.assertEqual(vision_chat_media_type("md"), "text/markdown")
        with self.assertRaisesRegex(ValueError, "unsupported vision chat output format"):
            normalize_vision_chat_output_format("xml")

    def test_mlx_vlm_backend_maps_request_to_runtime_package(self) -> None:
        calls: dict[str, object] = {}

        def fake_load(path: str) -> tuple[FakeMlxModel, str]:
            calls["load_path"] = path
            return FakeMlxModel(), "processor"

        def fake_apply_chat_template(
            processor: object,
            config: object,
            prompt: str,
            *,
            num_images: int,
        ) -> str:
            calls["template"] = (processor, config, prompt, num_images)
            return "formatted prompt"

        def fake_generate(
            model: object,
            processor: object,
            prompt: str,
            images: list[str],
            **kwargs: object,
        ) -> FakeGenerationResult:
            calls["generate"] = (model, processor, prompt, images, kwargs)
            return FakeGenerationResult(" a tiny cat <end_of_utterance>")

        deps = MlxVlmDeps(
            load=fake_load,
            generate=fake_generate,
            apply_chat_template=fake_apply_chat_template,
            load_config=lambda _path: "unused",
        )
        with patch(
            "tentgent_daemon.backends.mlx_vlm._load_mlx_vlm_deps",
            return_value=deps,
        ):
            backend = MlxVlmVisionChatBackend()

        root = Path("/tmp/tentgent-mlx-vlm-test/model")
        backend.load(stored_model_record(root))
        result = backend.generate_vision_chat(
            VisionChatRequest(
                model_ref="model-ref",
                image_path=Path("/tmp/tentgent-mlx-vlm-test/image.png"),
                prompt="describe",
                system_prompt="be brief",
                output_format="text",
                max_tokens=32,
                temperature=0.1,
            )
        )

        self.assertEqual(calls["load_path"], str(root / "variants" / "mlx" / "source"))
        self.assertEqual(
            calls["template"],
            ("processor", "model-config", "be brief\n\ndescribe", 1),
        )
        self.assertEqual(
            calls["generate"],
            (
                calls["generate"][0],
                "processor",
                "formatted prompt",
                ["/tmp/tentgent-mlx-vlm-test/image.png"],
                {"verbose": False, "max_tokens": 32, "temperature": 0.1},
            ),
        )
        self.assertEqual(result.text, "a tiny cat")
        self.assertEqual(result.media_type, "text/plain")

    def test_backend_factory_creates_mlx_vlm_backend(self) -> None:
        deps = MlxVlmDeps(
            load=lambda _path: (FakeMlxModel(), "processor"),
            generate=lambda *_args, **_kwargs: "ok",
            apply_chat_template=lambda *_args, **_kwargs: "prompt",
            load_config=lambda _path: "config",
        )
        with patch(
            "tentgent_daemon.backends.mlx_vlm._load_mlx_vlm_deps",
            return_value=deps,
        ):
            backend = create_vision_chat_backend(BackendKind.MLX_VLM)

        self.assertIsInstance(backend, MlxVlmVisionChatBackend)


def write_model_record(home: Path, model_ref: str, capabilities: list[str]) -> None:
    store_dir = home / "models" / "store" / model_ref
    (store_dir / "variants" / "safetensors" / "source").mkdir(parents=True)
    capabilities_toml = ", ".join(f'"{capability}"' for capability in capabilities)
    (store_dir / "model.toml").write_text(
        f"""
model_ref = "{model_ref}"
short_ref = "{model_ref[:12]}"
source_kind = "local"
source_path = "{home / "fixtures" / "model"}"
primary_format = "safetensors"
detected_formats = ["safetensors"]
model_capabilities = [{capabilities_toml}]
model_capability_source = "explicit-user"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
""",
        encoding="utf-8",
    )
    (store_dir / "manifest.json").write_text("{}", encoding="utf-8")


class FakeMlxModel:
    config = "model-config"


class FakeGenerationResult:
    def __init__(self, text: str) -> None:
        self.text = text


def stored_model_record(root: Path) -> StoredModelRecord:
    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="huggingface",
        source_repo="mlx-community/demo",
        source_revision="main",
        source_path=None,
        primary_format="mlx",
        detected_formats=("mlx",),
        mlx_runtime_family="mlx-vlm",
        model_capabilities=("vision-chat",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-05-20T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / "mlx" / "source",
    )


if __name__ == "__main__":
    unittest.main()
