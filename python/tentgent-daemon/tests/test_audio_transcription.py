from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tentgent_daemon.backends import create_audio_transcription_backend
from tentgent_daemon.backends.mlx_audio import MlxAudioDeps, MlxAudioTranscriptionBackend
from tentgent_daemon.runtime.audio import (
    AudioTranscriptionRequest,
    audio_transcription_media_type,
    build_audio_transcription_plan,
    normalize_audio_transcription_output_format,
    render_audio_transcription_output,
    write_audio_transcription_output,
)
from tentgent_daemon.runtime.records import StoredModelRecord
from tentgent_daemon.runtime.router import BackendKind


class AudioTranscriptionRuntimeTests(unittest.TestCase):
    def test_build_plan_accepts_audio_safetensors_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "a" * 64
            write_model_record(home, model_ref, ["audio-transcription"])
            input_path = home / "fixtures" / "audio.wav"
            input_path.parent.mkdir(parents=True)
            input_path.write_bytes(b"audio")

            plan = build_audio_transcription_plan(
                AudioTranscriptionRequest(
                    model_ref=model_ref[:12],
                    input_path=input_path,
                    output_path=home / "out" / "transcript.txt",
                    output_format="txt",
                    language="en",
                    timestamps=False,
                ),
                home=home,
            )

            self.assertEqual(plan.record.model_ref, model_ref)
            self.assertEqual(plan.request.output_format, "text")
            self.assertEqual(plan.request.input_path, input_path.resolve())
            self.assertEqual(plan.load_path.name, "source")

    def test_build_plan_rejects_non_audio_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "b" * 64
            write_model_record(home, model_ref, ["embedding"])
            input_path = home / "audio.wav"
            input_path.write_bytes(b"audio")

            with self.assertRaisesRegex(ValueError, "audio-transcription"):
                build_audio_transcription_plan(
                    AudioTranscriptionRequest(
                        model_ref=model_ref,
                        input_path=input_path,
                        output_path=home / "transcript.txt",
                        output_format="text",
                    ),
                    home=home,
                )

    def test_render_output_formats(self) -> None:
        raw = {
            "text": "hello world",
            "chunks": [
                {"timestamp": (0.0, 1.25), "text": "hello"},
                {"timestamp": (1.25, 2.5), "text": "world"},
            ],
        }

        text, text_value = render_audio_transcription_output(raw, "text")
        self.assertEqual(text, b"hello world\n")
        self.assertEqual(text_value, "hello world")

        payload, _ = render_audio_transcription_output(raw, "json")
        self.assertEqual(json.loads(payload.decode("utf-8"))["text"], "hello world")

        vtt, _ = render_audio_transcription_output(raw, "vtt")
        self.assertIn(b"WEBVTT", vtt)
        self.assertIn(b"00:00:00.000 --> 00:00:01.250", vtt)

        srt, _ = render_audio_transcription_output(raw, "srt")
        self.assertIn(b"1\n00:00:00,000 --> 00:00:01,250", srt)

    def test_write_output_creates_parent_and_reports_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_path = Path(tmp) / "nested" / "transcript.vtt"
            result = write_audio_transcription_output(
                AudioTranscriptionRequest(
                    model_ref="model",
                    input_path=Path(tmp) / "audio.wav",
                    output_path=output_path,
                    output_format="vtt",
                ),
                {
                    "text": "hello",
                    "chunks": [
                        {"timestamp": (0.0, 1.0), "text": "hello"},
                    ],
                },
            )

            self.assertTrue(output_path.is_file())
            self.assertEqual(result.media_type, "text/vtt")
            self.assertEqual(result.output_path, output_path)
            self.assertEqual(result.total_bytes, len(output_path.read_bytes()))
            self.assertEqual(result.text, "hello")

    def test_subtitle_output_requires_segment_timestamps(self) -> None:
        with self.assertRaisesRegex(ValueError, "requires segment timestamps"):
            render_audio_transcription_output({"text": "hello"}, "vtt")

        with self.assertRaisesRegex(ValueError, "requires segment timestamps"):
            render_audio_transcription_output(
                {
                    "text": "hello",
                    "chunks": [{"text": "hello", "timestamp": (0.0, None)}],
                },
                "srt",
            )

    def test_output_format_helpers_validate_values(self) -> None:
        self.assertEqual(normalize_audio_transcription_output_format("txt"), "text")
        self.assertEqual(audio_transcription_media_type("srt"), "application/x-subrip")
        with self.assertRaisesRegex(ValueError, "unsupported audio transcription output format"):
            normalize_audio_transcription_output_format("pdf")

    def test_mlx_audio_backend_maps_request_to_runtime_package(self) -> None:
        calls: dict[str, object] = {}

        class FakeMlxAudioModel:
            def generate(
                self,
                audio: str,
                *,
                language: str | None = None,
                word_timestamps: bool = False,
            ) -> dict[str, object]:
                calls["generate"] = {
                    "audio": audio,
                    "language": language,
                    "word_timestamps": word_timestamps,
                }
                return {
                    "text": " hello world ",
                    "segments": [
                        {"text": "hello", "start": 0.0, "end": 1.25},
                        {"text": "world", "start": 1.25, "end": 2.5},
                    ],
                }

        def fake_load(path: str) -> FakeMlxAudioModel:
            calls["load_path"] = path
            return FakeMlxAudioModel()

        with patch(
            "tentgent_daemon.backends.mlx_audio._load_mlx_audio_deps",
            return_value=MlxAudioDeps(load=fake_load),
        ):
            backend = MlxAudioTranscriptionBackend()

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output_path = root / "out" / "transcript.vtt"
            input_path = root / "audio.wav"
            input_path.write_bytes(b"audio")
            backend.load(stored_mlx_audio_record(root))
            result = backend.transcribe(
                AudioTranscriptionRequest(
                    model_ref="model-ref",
                    input_path=input_path,
                    output_path=output_path,
                    output_format="vtt",
                    language="en",
                    timestamps=True,
                )
            )

            self.assertEqual(
                calls["load_path"],
                str(root / "variants" / "mlx" / "source"),
            )
            self.assertEqual(
                calls["generate"],
                {
                    "audio": str(input_path),
                    "language": "en",
                    "word_timestamps": True,
                },
            )
            self.assertEqual(result.text, "hello world")
            self.assertEqual(result.media_type, "text/vtt")
            self.assertIn("WEBVTT", output_path.read_text(encoding="utf-8"))

    def test_mlx_audio_backend_reports_missing_subtitle_timestamps(self) -> None:
        class FakeMlxAudioModel:
            def generate(self, audio: str) -> dict[str, object]:
                return {"text": "hello"}

        with patch(
            "tentgent_daemon.backends.mlx_audio._load_mlx_audio_deps",
            return_value=MlxAudioDeps(load=lambda _path: FakeMlxAudioModel()),
        ):
            backend = MlxAudioTranscriptionBackend()

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "audio.wav"
            input_path.write_bytes(b"audio")
            backend.load(stored_mlx_audio_record(root))
            with self.assertRaisesRegex(ValueError, "requires segment timestamps"):
                backend.transcribe(
                    AudioTranscriptionRequest(
                        model_ref="model-ref",
                        input_path=input_path,
                        output_path=root / "transcript.srt",
                        output_format="srt",
                    )
                )

    def test_mlx_audio_backend_explains_missing_processor_metadata(self) -> None:
        class FakeMlxAudioModel:
            def generate(self, audio: str) -> dict[str, object]:
                raise ValueError(
                    "Processor not found. Make sure the model was loaded with a "
                    "HuggingFace processor."
                )

        with patch(
            "tentgent_daemon.backends.mlx_audio._load_mlx_audio_deps",
            return_value=MlxAudioDeps(load=lambda _path: FakeMlxAudioModel()),
        ):
            backend = MlxAudioTranscriptionBackend()

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            input_path = root / "audio.wav"
            input_path.write_bytes(b"audio")
            backend.load(stored_mlx_audio_record(root))
            with self.assertRaisesRegex(RuntimeError, "missing Hugging Face processor"):
                backend.transcribe(
                    AudioTranscriptionRequest(
                        model_ref="model-ref",
                        input_path=input_path,
                        output_path=root / "transcript.txt",
                        output_format="text",
                    )
                )

    def test_backend_factory_creates_mlx_audio_backend(self) -> None:
        with patch(
            "tentgent_daemon.backends.mlx_audio._load_mlx_audio_deps",
            return_value=MlxAudioDeps(load=lambda _path: object()),
        ):
            backend = create_audio_transcription_backend(BackendKind.MLX_AUDIO)

        self.assertIsInstance(backend, MlxAudioTranscriptionBackend)


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


def stored_mlx_audio_record(root: Path) -> StoredModelRecord:
    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="huggingface",
        source_repo="mlx-community/demo",
        source_revision="main",
        source_path=None,
        primary_format="mlx",
        detected_formats=("mlx",),
        mlx_runtime_family="mlx-audio",
        model_capabilities=("audio-transcription",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-05-20T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / "mlx" / "source",
    )


if __name__ == "__main__":
    unittest.main()
