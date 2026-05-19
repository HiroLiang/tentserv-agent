from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

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


if __name__ == "__main__":
    unittest.main()
