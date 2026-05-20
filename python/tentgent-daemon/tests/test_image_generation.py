from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tentgent_daemon.backends.diffusers import _diffusers_load_kwargs
from tentgent_daemon.backends.mlx_diffusion import (
    MfluxDeps,
    MfluxImageGenerationBackend,
    _mflux_base_model,
    _mflux_quantize_bits,
)
from tentgent_daemon.backends import create_image_generation_backend
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

    def test_build_plan_accepts_mlx_diffusion_image_generation_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "a" * 64
            write_model_record(
                home,
                model_ref,
                ["image-generation"],
                primary_format="mlx",
                detected_formats=["mlx"],
                mlx_runtime_family="mlx-diffusion",
                source_repo="mlx-community/Flux-1.lite-8B-MLX-Q4",
            )

            with patch(
                "tentgent_daemon.runtime.router.ensure_backend_supported"
            ) as ensure:
                plan = build_image_generation_plan(
                    ImageGenerationRequest(
                        model_ref=model_ref[:12],
                        prompt="a tiny red square",
                        output_path=home / "out" / "image.png",
                        output_format="png",
                    ),
                    home=home,
                )

            self.assertEqual(plan.backend, "mlx_diffusion")
            self.assertEqual(plan.record.mlx_runtime_family, "mlx-diffusion")
            self.assertEqual(plan.load_path.name, "source")
            ensure.assert_called_once_with("mlx_diffusion")

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

    def test_mflux_backend_maps_request_to_flux_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "b" * 64
            write_model_record(
                home,
                model_ref,
                ["image-generation"],
                primary_format="mlx",
                detected_formats=["mlx"],
                mlx_runtime_family="mlx-diffusion",
                source_repo="mlx-community/Flux-1.lite-8B-MLX-Q4",
            )
            with patch("tentgent_daemon.runtime.router.ensure_backend_supported"):
                plan = build_image_generation_plan(
                    ImageGenerationRequest(
                        model_ref=model_ref,
                        prompt="draw",
                        negative_prompt="blur",
                        output_path=home / "image.png",
                        output_format="png",
                        width=128,
                        height=128,
                        steps=2,
                        guidance_scale=4.0,
                        seed=11,
                    ),
                    home=home,
                )

            with patch(
                "tentgent_daemon.backends.mlx_diffusion._load_mflux_deps",
                return_value=MfluxDeps(Flux1=FakeFlux1, ModelConfig=FakeModelConfig),
            ):
                backend = MfluxImageGenerationBackend()
                backend.load(plan.record)
                result = backend.generate_image(plan.request)

            self.assertEqual(FakeFlux1.observed_model_path, plan.record.variant_source_path)
            self.assertEqual(FakeFlux1.observed_quantize, 4)
            self.assertEqual(FakeModelConfig.observed, ("mlx-community/Flux-1.lite-8B-MLX-Q4", "schnell"))
            self.assertEqual(FakeFlux1.observed_generate["prompt"], "draw")
            self.assertEqual(FakeFlux1.observed_generate["negative_prompt"], "blur")
            self.assertEqual(FakeFlux1.observed_generate["width"], 128)
            self.assertEqual(FakeFlux1.observed_generate["height"], 128)
            self.assertEqual(FakeFlux1.observed_generate["num_inference_steps"], 2)
            self.assertEqual(FakeFlux1.observed_generate["guidance"], 4.0)
            self.assertEqual(FakeFlux1.observed_generate["seed"], 11)
            self.assertTrue(plan.request.output_path.is_file())
            self.assertEqual(result.media_type, "image/png")
            self.assertEqual(result.seed, 11)

    def test_mflux_backend_requires_source_repo_for_model_family(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "c" * 64
            write_model_record(
                home,
                model_ref,
                ["image-generation"],
                primary_format="mlx",
                detected_formats=["mlx"],
                mlx_runtime_family="mlx-diffusion",
                source_repo=None,
            )
            with patch("tentgent_daemon.runtime.router.ensure_backend_supported"):
                record = build_image_generation_plan(
                    ImageGenerationRequest(
                        model_ref=model_ref,
                        prompt="draw",
                        output_path=home / "image.png",
                        output_format="png",
                    ),
                    home=home,
                ).record

            with patch(
                "tentgent_daemon.backends.mlx_diffusion._load_mflux_deps",
                return_value=MfluxDeps(Flux1=FakeFlux1, ModelConfig=FakeModelConfig),
            ):
                backend = MfluxImageGenerationBackend()
                with self.assertRaisesRegex(RuntimeError, "source repo metadata"):
                    backend.load(record)

    def test_mflux_helpers_infer_supported_flux_metadata(self) -> None:
        self.assertEqual(_mflux_base_model("mlx-community/Flux-1.lite-8B-MLX-Q4"), "schnell")
        self.assertEqual(_mflux_base_model("org/FLUX.1-dev-MLX-Q4"), "dev")
        self.assertEqual(_mflux_base_model("org/unknown"), None)
        record = stored_model_record(
            source_repo="mlx-community/Flux-1.lite-8B-MLX-Q4",
            source_path=None,
            variant_source_path=Path("/tmp/model/variants/mlx/source"),
        )
        self.assertEqual(_mflux_quantize_bits(record), 4)

    def test_backend_factory_creates_mflux_backend(self) -> None:
        with patch(
            "tentgent_daemon.backends.mlx_diffusion._load_mflux_deps",
            return_value=MfluxDeps(Flux1=FakeFlux1, ModelConfig=FakeModelConfig),
        ):
            backend = create_image_generation_backend("mlx_diffusion")

        self.assertIsInstance(backend, MfluxImageGenerationBackend)


class FakeImage:
    def __init__(self, body: bytes) -> None:
        self.body = body

    def save(self, path: Path, **_kwargs: object) -> None:
        path.write_bytes(self.body)


class FakeGeneratedImage:
    def save(self, path: Path, **_kwargs: object) -> None:
        path.write_bytes(b"mflux-image")


class FakeFlux1:
    observed_model_path: Path | None = None
    observed_quantize: int | None = None
    observed_generate: dict[str, object] = {}

    def __init__(
        self,
        *,
        model_config: object,
        quantize: int | None,
        model_path: str,
    ) -> None:
        self.model_config = model_config
        FakeFlux1.observed_model_path = Path(model_path)
        FakeFlux1.observed_quantize = quantize

    def generate_image(self, **kwargs: object) -> FakeGeneratedImage:
        FakeFlux1.observed_generate = kwargs
        return FakeGeneratedImage()


class FakeModelConfig:
    observed: tuple[str, str | None] | None = None

    @staticmethod
    def from_name(model_name: str, base_model: str | None) -> object:
        FakeModelConfig.observed = (model_name, base_model)
        return {"model_name": model_name, "base_model": base_model}


class FakeDevice:
    def __init__(self, device_type: str) -> None:
        self.type = device_type


class FakeTorch:
    float16 = "float16"
    float32 = "float32"


def write_model_record(
    home: Path,
    model_ref: str,
    capabilities: list[str],
    *,
    primary_format: str = "diffusers",
    detected_formats: list[str] | None = None,
    mlx_runtime_family: str | None = None,
    source_repo: str | None = "org/model",
) -> None:
    store_dir = home / "models" / "store" / model_ref
    source = store_dir / "variants" / primary_format / "source"
    source.mkdir(parents=True)
    if primary_format == "diffusers":
        (source / "model_index.json").write_text("{}", encoding="utf-8")
    capabilities_toml = ", ".join(f'"{capability}"' for capability in capabilities)
    detected_formats = detected_formats or [primary_format]
    detected_formats_toml = ", ".join(f'"{format}"' for format in detected_formats)
    source_repo_toml = (
        f'source_repo = "{source_repo}"\n' if source_repo is not None else ""
    )
    mlx_runtime_family_toml = (
        f'mlx_runtime_family = "{mlx_runtime_family}"\n'
        if mlx_runtime_family is not None
        else ""
    )
    (store_dir / "model.toml").write_text(
        f"""
model_ref = "{model_ref}"
short_ref = "{model_ref[:12]}"
source_kind = "local"
source_path = "{home / "fixtures" / "model"}"
{source_repo_toml}primary_format = "{primary_format}"
detected_formats = [{detected_formats_toml}]
{mlx_runtime_family_toml}model_capabilities = [{capabilities_toml}]
model_capability_source = "explicit-user"
file_count = 1
total_bytes = 10
imported_at = "2026-05-01T00:00:00Z"
""",
        encoding="utf-8",
    )
    (store_dir / "manifest.json").write_text("{}", encoding="utf-8")


def stored_model_record(
    *,
    source_repo: str | None,
    source_path: str | None,
    variant_source_path: Path,
):
    from tentgent_daemon.runtime.records import StoredModelRecord

    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="huggingface",
        source_repo=source_repo,
        source_revision="main",
        source_path=source_path,
        primary_format="mlx",
        detected_formats=("mlx",),
        mlx_runtime_family="mlx-diffusion",
        model_capabilities=("image-generation",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-05-21T00:00:00Z",
        store_path=variant_source_path.parent.parent.parent,
        manifest_path=variant_source_path.parent.parent.parent / "manifest.json",
        variant_source_path=variant_source_path,
    )


if __name__ == "__main__":
    unittest.main()
