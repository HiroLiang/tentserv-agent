from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tentgent_daemon.runtime.audio import (
    AudioTranscriptionRequest,
    audio_transcription_media_type,
    build_audio_transcription_plan,
    normalize_audio_transcription_output_format,
    render_audio_transcription_output,
    write_audio_transcription_output,
)


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
