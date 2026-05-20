from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tentgent_daemon.backends.diffusers import _diffusers_load_kwargs
from tentgent_daemon.runtime.image_generation import (
    ImageGenerationRequest,
    build_image_generation_plan,
    image_generation_media_type,
    normalize_image_generation_output_format,
    validate_image_generation_dimensions,
    write_image_generation_output,
)


class ImageGenerationRuntimeTests(unittest.TestCase):
    def test_build_plan_accepts_diffusers_image_generation_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "e" * 64
            write_model_record(home, model_ref, ["image-generation"])

            plan = build_image_generation_plan(
                ImageGenerationRequest(
                    model_ref=model_ref[:12],
                    prompt=" a tiny landscape ",
                    negative_prompt=" blurry ",
                    output_path=home / "out" / "image.png",
                    output_format="png",
                    width=512,
                    height=768,
                    steps=18,
                    guidance_scale=6.0,
                    seed=123,
                ),
                home=home,
            )

            self.assertEqual(plan.record.model_ref, model_ref)
            self.assertEqual(plan.backend, "diffusers")
            self.assertEqual(plan.request.prompt, "a tiny landscape")
            self.assertEqual(plan.request.negative_prompt, "blurry")
            self.assertEqual(plan.request.output_format, "png")
            self.assertEqual(plan.request.width, 512)
            self.assertEqual(plan.request.height, 768)
            self.assertEqual(plan.load_path.name, "source")

    def test_build_plan_rejects_non_image_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "f" * 64
            write_model_record(home, model_ref, ["vision-chat"])

            with self.assertRaisesRegex(ValueError, "image-generation"):
                build_image_generation_plan(
                    ImageGenerationRequest(
                        model_ref=model_ref,
                        prompt="draw",
                        output_path=home / "image.png",
                        output_format="png",
                    ),
                    home=home,
                )

    def test_build_plan_rejects_existing_output_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "1" * 64
            write_model_record(home, model_ref, ["image-generation"])
            output_path = home / "image.png"
            output_path.write_bytes(b"existing")

            with self.assertRaises(FileExistsError):
                build_image_generation_plan(
                    ImageGenerationRequest(
                        model_ref=model_ref,
                        prompt="draw",
                        output_path=output_path,
                        output_format="png",
                    ),
                    home=home,
                )

    def test_diffusers_loader_disables_missing_safety_checker(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "3" * 64
            write_model_record(home, model_ref, ["image-generation"])
            source = (
                home
                / "models"
                / "store"
                / model_ref
                / "variants"
                / "diffusers"
                / "source"
            )
            (source / "model_index.json").write_text(
                '{"safety_checker": ["stable_diffusion", "StableDiffusionSafetyChecker"]}',
                encoding="utf-8",
            )

            plan = build_image_generation_plan(
                ImageGenerationRequest(
                    model_ref=model_ref,
                    prompt="draw",
                    output_path=home / "image.png",
                    output_format="png",
                ),
                home=home,
            )

            self.assertEqual(
                _diffusers_load_kwargs(plan.record, FakeTorch, FakeDevice("cpu")),
                {"safety_checker": None, "torch_dtype": "float32"},
            )

    def test_diffusers_loader_uses_stable_default_dtypes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "4" * 64
            write_model_record(home, model_ref, ["image-generation"])
            plan = build_image_generation_plan(
                ImageGenerationRequest(
                    model_ref=model_ref,
                    prompt="draw",
                    output_path=home / "image.png",
                    output_format="png",
                ),
                home=home,
            )

            self.assertEqual(
                _diffusers_load_kwargs(plan.record, FakeTorch, FakeDevice("cpu"))[
                    "torch_dtype"
                ],
                "float32",
            )
            self.assertEqual(
                _diffusers_load_kwargs(plan.record, FakeTorch, FakeDevice("mps"))[
                    "torch_dtype"
                ],
                "float32",
            )
            self.assertEqual(
                _diffusers_load_kwargs(plan.record, FakeTorch, FakeDevice("cuda"))[
                    "torch_dtype"
                ],
                "float16",
            )
            with patch.dict(
                "os.environ", {"TENTGENT_IMAGE_GENERATION_TORCH_DTYPE": "float32"}
            ):
                self.assertEqual(
                    _diffusers_load_kwargs(plan.record, FakeTorch, FakeDevice("cuda"))[
                        "torch_dtype"
                    ],
                    "float32",
                )

    def test_build_plan_rejects_oversized_prompt(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "2" * 64
            write_model_record(home, model_ref, ["image-generation"])

            with self.assertRaisesRegex(ValueError, "at most 8192 bytes"):
                build_image_generation_plan(
                    ImageGenerationRequest(
                        model_ref=model_ref,
                        prompt="x" * 8193,
                        output_path=home / "image.png",
                        output_format="png",
                    ),
                    home=home,
                )

    def test_output_format_and_dimension_helpers_validate_values(self) -> None:
        self.assertEqual(normalize_image_generation_output_format("jpeg"), "jpg")
        self.assertEqual(image_generation_media_type("jpg"), "image/jpeg")
        validate_image_generation_dimensions(512, 512)
        with self.assertRaisesRegex(ValueError, "unsupported image generation output format"):
            normalize_image_generation_output_format("webp")
        with self.assertRaisesRegex(ValueError, "divisible by 8"):
            validate_image_generation_dimensions(513, 512)

    def test_write_image_output_creates_parent_and_reports_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_path = Path(tmp) / "nested" / "image.png"
            result = write_image_generation_output(
                ImageGenerationRequest(
                    model_ref="model",
                    prompt="draw",
                    output_path=output_path,
                    output_format="png",
                    width=128,
                    height=128,
                    seed=7,
                ),
                FakeImage(b"png-bytes"),
            )

            self.assertTrue(output_path.is_file())
            self.assertEqual(result.media_type, "image/png")
            self.assertEqual(result.output_path, output_path)
            self.assertEqual(result.total_bytes, len(output_path.read_bytes()))
            self.assertEqual(result.seed, 7)


class FakeImage:
    def __init__(self, body: bytes) -> None:
        self.body = body

    def save(self, path: Path, **_kwargs: object) -> None:
        path.write_bytes(self.body)


class FakeDevice:
    def __init__(self, device_type: str) -> None:
        self.type = device_type


class FakeTorch:
    float16 = "float16"
    float32 = "float32"


def write_model_record(home: Path, model_ref: str, capabilities: list[str]) -> None:
    store_dir = home / "models" / "store" / model_ref
    (store_dir / "variants" / "diffusers" / "source").mkdir(parents=True)
    (store_dir / "variants" / "diffusers" / "source" / "model_index.json").write_text(
        "{}",
        encoding="utf-8",
    )
    capabilities_toml = ", ".join(f'"{capability}"' for capability in capabilities)
    (store_dir / "model.toml").write_text(
        f"""
model_ref = "{model_ref}"
short_ref = "{model_ref[:12]}"
source_kind = "local"
source_path = "{home / "fixtures" / "model"}"
primary_format = "diffusers"
detected_formats = ["diffusers"]
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
