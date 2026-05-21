from __future__ import annotations

import tempfile
import unittest
import wave
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from tentgent_daemon.backends import create_audio_speech_backend
from tentgent_daemon.backends.mlx_audio import MlxAudioDeps, MlxAudioSpeechBackend
from tentgent_daemon.backends.transformers_peft import TransformersPeftAudioSpeechBackend
from tentgent_daemon.runtime.audio_speech import (
    AudioSpeechRequest,
    build_audio_speech_plan,
    normalize_audio_speech_output_format,
    validate_audio_speech_text,
    write_audio_speech_output,
)
from tentgent_daemon.runtime.records import StoredModelRecord
from tentgent_daemon.runtime.router import BackendKind


class AudioSpeechRuntimeTests(unittest.TestCase):
    def test_build_plan_accepts_audio_speech_safetensors_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "a" * 64
            write_model_record(home, model_ref, ["audio-speech"])

            plan = build_audio_speech_plan(
                AudioSpeechRequest(
                    model_ref=model_ref[:12],
                    text=" hello ",
                    output_path=home / "out" / "speech.wav",
                    output_format="wave",
                    language="en",
                    voice="default",
                ),
                home=home,
            )

            self.assertEqual(plan.record.model_ref, model_ref)
            self.assertEqual(plan.request.text, "hello")
            self.assertEqual(plan.request.output_format, "wav")
            self.assertEqual(plan.request.output_path, (home / "out" / "speech.wav").resolve())
            self.assertEqual(plan.load_path.name, "source")

    def test_build_plan_rejects_non_speech_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "b" * 64
            write_model_record(home, model_ref, ["audio-transcription"])

            with self.assertRaisesRegex(ValueError, "audio-speech"):
                build_audio_speech_plan(
                    AudioSpeechRequest(
                        model_ref=model_ref,
                        text="hello",
                        output_path=home / "speech.wav",
                        output_format="wav",
                    ),
                    home=home,
                )

    def test_output_format_and_text_helpers_validate_values(self) -> None:
        self.assertEqual(normalize_audio_speech_output_format("wave"), "wav")
        with self.assertRaisesRegex(ValueError, "unsupported audio speech output format"):
            normalize_audio_speech_output_format("mp3")
        with self.assertRaisesRegex(ValueError, "must not be empty"):
            validate_audio_speech_text(" ")

    def test_write_output_creates_wav_and_reports_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_path = Path(tmp) / "nested" / "speech.wav"
            result = write_audio_speech_output(
                AudioSpeechRequest(
                    model_ref="model",
                    text="hello",
                    output_path=output_path,
                    output_format="wav",
                ),
                {
                    "audio": [-0.25, 0.0, 0.25],
                    "sampling_rate": 16000,
                },
            )

            self.assertTrue(output_path.is_file())
            self.assertEqual(result.media_type, "audio/wav")
            self.assertEqual(result.sample_rate, 16000)
            self.assertEqual(result.total_bytes, output_path.stat().st_size)
            with wave.open(str(output_path), "rb") as handle:
                self.assertEqual(handle.getnchannels(), 1)
                self.assertEqual(handle.getsampwidth(), 2)
                self.assertEqual(handle.getframerate(), 16000)
                self.assertEqual(handle.getnframes(), 3)

    def test_transformers_audio_speech_backend_maps_request_to_pipeline(self) -> None:
        calls: dict[str, object] = {}

        def fake_pipeline(task: str, **kwargs: object):
            calls["load"] = {"task": task, **kwargs}

            def run(text: str, **run_kwargs: object) -> dict[str, object]:
                calls["run"] = {"text": text, **run_kwargs}
                return {"audio": [0.0, 0.1, -0.1], "sampling_rate": 8000}

            return run

        with patch(
            "tentgent_daemon.backends.transformers_peft._load_transformers_peft_deps",
            return_value=SimpleNamespace(torch=FakeTorch(), pipeline=fake_pipeline),
        ):
            backend = TransformersPeftAudioSpeechBackend()

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output_path = root / "speech.wav"
            backend.load(stored_safetensors_record(root))
            result = backend.synthesize_speech(
                AudioSpeechRequest(
                    model_ref="model-ref",
                    text="hello",
                    output_path=output_path,
                    output_format="wav",
                    language="en",
                    voice="default",
                )
            )

            self.assertEqual(calls["load"]["task"], "text-to-speech")
            self.assertEqual(calls["run"], {"text": "hello", "language": "en", "voice": "default"})
            self.assertEqual(result.media_type, "audio/wav")
            self.assertEqual(result.sample_rate, 8000)

    def test_mlx_audio_speech_backend_reports_planned_runtime(self) -> None:
        with patch(
            "tentgent_daemon.backends.mlx_audio._load_mlx_audio_deps",
            return_value=MlxAudioDeps(load=lambda _path: object()),
        ):
            backend = MlxAudioSpeechBackend()

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            backend.load(stored_mlx_audio_record(root))
            with self.assertRaisesRegex(RuntimeError, "planned but not implemented"):
                backend.synthesize_speech(
                    AudioSpeechRequest(
                        model_ref="model-ref",
                        text="hello",
                        output_path=root / "speech.wav",
                    )
                )

    def test_backend_factory_creates_audio_speech_backends(self) -> None:
        with patch(
            "tentgent_daemon.backends.transformers_peft._load_transformers_peft_deps",
            return_value=SimpleNamespace(torch=FakeTorch(), pipeline=lambda *_a, **_kw: object()),
        ):
            transformers_backend = create_audio_speech_backend(BackendKind.TRANSFORMERS_PEFT)
        self.assertIsInstance(transformers_backend, TransformersPeftAudioSpeechBackend)

        with patch(
            "tentgent_daemon.backends.mlx_audio._load_mlx_audio_deps",
            return_value=MlxAudioDeps(load=lambda _path: object()),
        ):
            mlx_backend = create_audio_speech_backend(BackendKind.MLX_AUDIO)
        self.assertIsInstance(mlx_backend, MlxAudioSpeechBackend)


class FakeTorch:
    class cuda:
        @staticmethod
        def is_available() -> bool:
            return False

        @staticmethod
        def empty_cache() -> None:
            return None

    class backends:
        class mps:
            @staticmethod
            def is_available() -> bool:
                return False

    class mps:
        @staticmethod
        def empty_cache() -> None:
            return None


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


def stored_safetensors_record(root: Path) -> StoredModelRecord:
    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="huggingface",
        source_repo="facebook/mms-tts-eng",
        source_revision="main",
        source_path=None,
        primary_format="safetensors",
        detected_formats=("safetensors",),
        mlx_runtime_family=None,
        model_capabilities=("audio-speech",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-05-21T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / "safetensors" / "source",
    )


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
        model_capabilities=("audio-speech",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-05-21T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / "mlx" / "source",
    )


if __name__ == "__main__":
    unittest.main()
