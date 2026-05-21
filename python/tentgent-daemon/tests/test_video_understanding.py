from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from tentgent_daemon.backends import create_video_understanding_backend
from tentgent_daemon.backends.mlx_vlm import (
    MlxVlmDeps,
    MlxVlmVideoUnderstandingBackend,
)
from tentgent_daemon.backends.transformers_peft import (
    TransformersPeftVideoUnderstandingBackend,
)
from tentgent_daemon.runtime.router import BackendKind
from tentgent_daemon.runtime.video_understanding import (
    VideoSamplingOptions,
    VideoUnderstandingRequest,
    build_video_understanding_plan,
    normalize_video_understanding_output_format,
    validate_video_sampling_options,
    video_understanding_media_type,
)


class VideoUnderstandingRuntimeTests(unittest.TestCase):
    def test_build_plan_accepts_video_safetensors_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "e" * 64
            write_model_record(home, model_ref, ["video-understanding"])
            video_path = home / "fixtures" / "video.mp4"
            video_path.parent.mkdir(parents=True)
            video_path.write_bytes(b"video")

            plan = build_video_understanding_plan(
                VideoUnderstandingRequest(
                    model_ref=model_ref[:12],
                    video_path=video_path,
                    prompt=" describe ",
                    system_prompt=" be brief ",
                    output_format="markdown",
                    max_tokens=32,
                    temperature=0.0,
                    sampling=VideoSamplingOptions(sample_fps=0.5, max_frames=4),
                ),
                home=home,
            )

            self.assertEqual(plan.record.model_ref, model_ref)
            self.assertEqual(plan.request.output_format, "md")
            self.assertEqual(plan.request.prompt, "describe")
            self.assertEqual(plan.request.system_prompt, "be brief")
            self.assertEqual(plan.request.video_path, video_path.resolve())
            self.assertEqual(plan.request.sampling.sample_fps, 0.5)
            self.assertEqual(plan.request.sampling.max_frames, 4)
            self.assertEqual(plan.load_path.name, "source")

    def test_build_plan_rejects_non_video_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "f" * 64
            write_model_record(home, model_ref, ["vision-chat"])
            video_path = home / "video.mp4"
            video_path.write_bytes(b"video")

            with self.assertRaisesRegex(ValueError, "video-understanding"):
                build_video_understanding_plan(
                    VideoUnderstandingRequest(
                        model_ref=model_ref,
                        video_path=video_path,
                        prompt="describe",
                        output_format="text",
                    ),
                    home=home,
                )

    def test_output_format_and_sampling_helpers_validate_values(self) -> None:
        self.assertEqual(normalize_video_understanding_output_format("txt"), "text")
        self.assertEqual(normalize_video_understanding_output_format("markdown"), "md")
        self.assertEqual(video_understanding_media_type("json"), "application/json")
        with self.assertRaisesRegex(
            ValueError,
            "unsupported video understanding output format",
        ):
            normalize_video_understanding_output_format("xml")
        with self.assertRaisesRegex(ValueError, "sample_fps"):
            validate_video_sampling_options(VideoSamplingOptions(sample_fps=0.0))

    def test_backend_factory_creates_video_understanding_backends(self) -> None:
        with patch(
            "tentgent_daemon.backends.transformers_peft._load_transformers_peft_deps",
            return_value=SimpleNamespace(torch=FakeTorch()),
        ):
            transformers_backend = create_video_understanding_backend(
                BackendKind.TRANSFORMERS_PEFT
            )
        self.assertIsInstance(
            transformers_backend,
            TransformersPeftVideoUnderstandingBackend,
        )

        deps = MlxVlmDeps(
            load=lambda _path: (object(), "processor"),
            generate=lambda *_args, **_kwargs: "ok",
            apply_chat_template=lambda *_args, **_kwargs: "prompt",
            load_config=lambda _path: "config",
        )
        with patch(
            "tentgent_daemon.backends.mlx_vlm._load_mlx_vlm_deps",
            return_value=deps,
        ):
            mlx_backend = create_video_understanding_backend(BackendKind.MLX_VLM)
        self.assertIsInstance(mlx_backend, MlxVlmVideoUnderstandingBackend)


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


class FakeTorch:
    @staticmethod
    def device(name: str) -> str:
        return name

    class cuda:
        @staticmethod
        def is_available() -> bool:
            return False

    class backends:
        class mps:
            @staticmethod
            def is_available() -> bool:
                return False


if __name__ == "__main__":
    unittest.main()
