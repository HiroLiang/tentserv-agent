from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from tentgent_daemon.runtime.records import StoredModelRecord, load_model_record
from tentgent_daemon.runtime.router import (
    BackendKind,
    resolve_audio_transcription_backend,
    resolve_backend,
    resolve_image_generation_backend,
    resolve_vision_chat_backend,
)


class RuntimeRouterTests(unittest.TestCase):
    def test_records_read_optional_mlx_runtime_family(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "a" * 64
            write_model_record(home, model_ref, mlx_runtime_family="mlx-vlm")

            record = load_model_record(model_ref[:12], home=home)

            self.assertEqual(record.mlx_runtime_family, "mlx-vlm")

    def test_records_default_missing_mlx_runtime_family_to_none(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "b" * 64
            write_model_record(home, model_ref)

            record = load_model_record(model_ref[:12], home=home)

            self.assertIsNone(record.mlx_runtime_family)

    def test_chat_backend_rejects_non_lm_mlx_family(self) -> None:
        record = stored_model_record(
            primary_format="mlx",
            mlx_runtime_family="mlx-vlm",
            model_capabilities=("vision-chat",),
        )

        with self.assertRaisesRegex(ValueError, "expected `mlx-lm`"):
            resolve_backend(record)

    def test_media_mlx_families_are_recorded_but_not_implemented_yet(self) -> None:
        with self.assertRaisesRegex(ValueError, "planned and not implemented"):
            resolve_vision_chat_backend(
                stored_model_record(
                    primary_format="mlx",
                    mlx_runtime_family="mlx-vlm",
                    model_capabilities=("vision-chat",),
                )
            )
        with self.assertRaisesRegex(ValueError, "planned and not implemented"):
            resolve_audio_transcription_backend(
                stored_model_record(
                    primary_format="mlx",
                    mlx_runtime_family="mlx-audio",
                    model_capabilities=("audio-transcription",),
                )
            )
        with self.assertRaisesRegex(ValueError, "planned and not implemented"):
            resolve_image_generation_backend(
                stored_model_record(
                    primary_format="mlx",
                    mlx_runtime_family="mlx-diffusion",
                    model_capabilities=("image-generation",),
                )
            )

    def test_existing_media_backends_still_resolve(self) -> None:
        self.assertEqual(
            resolve_vision_chat_backend(
                stored_model_record(
                    primary_format="safetensors",
                    model_capabilities=("vision-chat",),
                )
            ),
            BackendKind.TRANSFORMERS_PEFT,
        )
        self.assertEqual(
            resolve_audio_transcription_backend(
                stored_model_record(
                    primary_format="safetensors",
                    model_capabilities=("audio-transcription",),
                )
            ),
            BackendKind.TRANSFORMERS_PEFT,
        )
        self.assertEqual(
            resolve_image_generation_backend(
                stored_model_record(
                    primary_format="diffusers",
                    model_capabilities=("image-generation",),
                )
            ),
            BackendKind.DIFFUSERS,
        )


def stored_model_record(
    *,
    primary_format: str,
    model_capabilities: tuple[str, ...],
    mlx_runtime_family: str | None = None,
) -> StoredModelRecord:
    root = Path("/tmp/tentgent-runtime-router-test/model")
    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="local",
        source_repo=None,
        source_revision=None,
        source_path=None,
        primary_format=primary_format,
        detected_formats=(primary_format,),
        mlx_runtime_family=mlx_runtime_family,
        model_capabilities=model_capabilities,
        file_count=1,
        total_bytes=1,
        imported_at="2026-05-20T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / primary_format / "source",
    )


def write_model_record(
    home: Path,
    model_ref: str,
    *,
    mlx_runtime_family: str | None = None,
) -> None:
    store_dir = home / "models" / "store" / model_ref
    (store_dir / "variants" / "mlx" / "source").mkdir(parents=True)
    family_line = (
        f'mlx_runtime_family = "{mlx_runtime_family}"\n'
        if mlx_runtime_family is not None
        else ""
    )
    (store_dir / "model.toml").write_text(
        f"""
model_ref = "{model_ref}"
short_ref = "{model_ref[:12]}"
source_kind = "huggingface"
source_repo = "mlx-community/demo"
source_revision = "main"
primary_format = "mlx"
detected_formats = ["mlx"]
{family_line}model_capabilities = ["vision-chat"]
model_capability_source = "explicit-user"
file_count = 1
total_bytes = 10
imported_at = "2026-05-20T00:00:00Z"
""",
        encoding="utf-8",
    )


if __name__ == "__main__":
    unittest.main()
