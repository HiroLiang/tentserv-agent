from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from fastapi import HTTPException

from tentgent.runtime.backends.audio_speech import AudioSpeechModelKind
from tentgent.runtime.backends.audio_transcription import AudioTranscriptionModelKind
from tentgent.runtime.backends.chat import ChatModelKind
from tentgent.runtime.backends.embedding import EmbeddingModelKind
from tentgent.runtime.backends.image_generation import (
    ImageGenerationModelKind,
    ImageGenerationWorkflowKind,
)
from tentgent.runtime.backends.records import ModelCapability, ModelFormat
from tentgent.runtime.backends.rerank import RerankModelKind
from tentgent.runtime.backends.video_understanding import VideoUnderstandingModelKind
from tentgent.runtime.backends.vision_chat import VisionChatModelKind
from tentgent.runtime.server.app import create_app
from tentgent.runtime.server.lifecycle import RuntimeCapability, RuntimeServerConfig
from tentgent.runtime.server.managed_models import load_managed_model_record
from tentgent.runtime.server.routes.audio_speech import (
    AudioSpeechPayload,
    _build_audio_speech_inference_request,
)
from tentgent.runtime.server.routes.audio_transcription import (
    AudioTranscriptionPayload,
    _build_audio_transcription_inference_request,
)
from tentgent.runtime.server.routes.chat import (
    ChatMessagePayload,
    ChatPayload,
    _build_chat_inference_request,
)
from tentgent.runtime.server.routes.embedding import (
    EmbeddingPayload,
    _build_embedding_inference_request,
)
from tentgent.runtime.server.routes.image_generation import (
    ImageGenerationPayload,
    _build_image_inference_request,
)
from tentgent.runtime.server.routes.payloads import ModelRecordPayload
from tentgent.runtime.server.routes.rerank import (
    RerankPayload,
    _build_rerank_inference_request,
)
from tentgent.runtime.server.routes.video_understanding import (
    VideoUnderstandingPayload,
    _build_video_understanding_inference_request,
)
from tentgent.runtime.server.routes.vision_chat import (
    VisionChatPayload,
    _build_vision_chat_inference_request,
)


class ManagedModelRecordTests(unittest.TestCase):
    def test_loads_managed_model_record_from_runtime_home(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "a" * 64
            source_path = _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.MLX,
                capability=ModelCapability.CHAT,
            )

            record = load_managed_model_record(model_ref[:12], home=home)

        self.assertEqual(record.model_ref, model_ref)
        self.assertEqual(record.short_ref, model_ref[:12])
        self.assertEqual(record.source_path, source_path.resolve())
        self.assertEqual(record.primary_format, ModelFormat.MLX)
        self.assertEqual(record.capabilities, frozenset({ModelCapability.CHAT}))


class ServerBoundRouteTests(unittest.TestCase):
    def test_chat_request_can_omit_model_when_server_is_model_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "b" * 64
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.MLX,
                capability=ModelCapability.CHAT,
            )
            payload = ChatPayload(
                messages=[ChatMessagePayload(role="user", content="Hello")],
                max_tokens=8,
                temperature=0.0,
            )

            _, inference = _build_chat_inference_request(
                payload,
                _request(home, RuntimeCapability.CHAT, model_ref),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(inference.model_kind, ChatModelKind.MLX)

    def test_embedding_request_can_omit_model_when_server_is_model_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "c" * 64
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.SAFETENSORS,
                capability=ModelCapability.EMBEDDING,
            )
            payload = EmbeddingPayload(input=["first", "second"])

            _, inference = _build_embedding_inference_request(
                payload,
                _request(home, RuntimeCapability.EMBEDDING, model_ref),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(inference.model_kind, EmbeddingModelKind.TRANSFORMERS)

    def test_rerank_request_can_omit_model_when_server_is_model_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "d" * 64
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.SAFETENSORS,
                capability=ModelCapability.RERANK,
            )
            payload = RerankPayload(
                query="refund policy",
                documents=["first", "second"],
                top_n=1,
            )

            _, inference = _build_rerank_inference_request(
                payload,
                _request(home, RuntimeCapability.RERANK, model_ref),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(inference.model_kind, RerankModelKind.TRANSFORMERS)

    def test_audio_transcription_request_can_omit_model_when_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "e" * 64
            input_path = home / "sample.wav"
            input_path.write_bytes(b"audio")
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.MLX,
                capability=ModelCapability.AUDIO_TRANSCRIPTION,
            )
            payload = AudioTranscriptionPayload(
                input_path=str(input_path),
                output_path=str(home / "sample.txt"),
            )

            _, inference = _build_audio_transcription_inference_request(
                payload,
                _request(home, RuntimeCapability.AUDIO_TRANSCRIPTION, model_ref),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(inference.model_kind, AudioTranscriptionModelKind.MLX_AUDIO)

    def test_audio_speech_request_can_omit_model_when_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "f" * 64
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.SAFETENSORS,
                capability=ModelCapability.AUDIO_SPEECH,
            )
            payload = AudioSpeechPayload(
                text="Hello from Tentgent.",
                output_path=str(home / "speech.wav"),
            )

            _, inference = _build_audio_speech_inference_request(
                payload,
                _request(home, RuntimeCapability.AUDIO_SPEECH, model_ref),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(inference.model_kind, AudioSpeechModelKind.TRANSFORMERS_TTS)

    def test_vision_chat_request_can_omit_model_when_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "1" * 64
            image_path = home / "image.png"
            image_path.write_bytes(b"image")
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.MLX,
                capability=ModelCapability.VISION_CHAT,
            )
            payload = VisionChatPayload(
                image_path=str(image_path),
                prompt="What is visible?",
            )

            _, inference = _build_vision_chat_inference_request(
                payload,
                _request(home, RuntimeCapability.VISION_CHAT, model_ref),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(inference.model_kind, VisionChatModelKind.MLX_VLM)

    def test_video_understanding_request_can_omit_model_when_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "2" * 64
            video_path = home / "clip.mp4"
            video_path.write_bytes(b"video")
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.SAFETENSORS,
                capability=ModelCapability.VIDEO_UNDERSTANDING,
            )
            payload = VideoUnderstandingPayload(
                video_path=str(video_path),
                prompt="Describe the clip.",
            )

            _, inference = _build_video_understanding_inference_request(
                payload,
                _request(home, RuntimeCapability.VIDEO_UNDERSTANDING, model_ref),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(
            inference.model_kind,
            VideoUnderstandingModelKind.TRANSFORMERS_VIDEO_UNDERSTANDING,
        )

    def test_image_generation_request_can_omit_model_when_bound(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "3" * 64
            _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.DIFFUSERS,
                capability=ModelCapability.IMAGE_GENERATION,
            )
            payload = ImageGenerationPayload(
                prompt="A small red cube",
                output_path=str(home / "cube.png"),
            )

            _, inference = _build_image_inference_request(
                payload,
                _request(home, RuntimeCapability.IMAGE_GENERATION, model_ref),
                workflow_kind=ImageGenerationWorkflowKind.TEXT_TO_IMAGE,
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(
            inference.model_kind,
            ImageGenerationModelKind.DIFFUSERS_TEXT_TO_IMAGE,
        )

    def test_direct_runtime_still_accepts_explicit_model_payload(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            model_ref = "4" * 64
            source_path = _write_model(
                home,
                model_ref=model_ref,
                format_=ModelFormat.MLX,
                capability=ModelCapability.CHAT,
            )
            payload = ChatPayload(
                model=ModelRecordPayload(
                    model_ref=model_ref,
                    source_path=str(source_path),
                    primary_format=ModelFormat.MLX,
                    capabilities=[ModelCapability.CHAT],
                ),
                messages=[ChatMessagePayload(role="user", content="Hello")],
            )

            _, inference = _build_chat_inference_request(
                payload,
                _direct_request(home, RuntimeCapability.CHAT),
            )

        self.assertEqual(inference.model.model_ref, model_ref)
        self.assertEqual(inference.model_kind, ChatModelKind.MLX)

    def test_direct_runtime_requires_model_payload(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            payload = ChatPayload(
                messages=[ChatMessagePayload(role="user", content="Hello")],
            )

            with self.assertRaises(HTTPException) as raised:
                _build_chat_inference_request(
                    payload,
                    _direct_request(home, RuntimeCapability.CHAT),
                )

        self.assertEqual(raised.exception.status_code, 400)
        self.assertIn("`model` is required", str(raised.exception.detail))

    def test_model_bound_runtime_rejects_different_explicit_model(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            bound_ref = "5" * 64
            other_ref = "6" * 64
            bound_source = _write_model(
                home,
                model_ref=bound_ref,
                format_=ModelFormat.MLX,
                capability=ModelCapability.CHAT,
            )
            _write_model(
                home,
                model_ref=other_ref,
                format_=ModelFormat.MLX,
                capability=ModelCapability.CHAT,
            )
            payload = ChatPayload(
                model=ModelRecordPayload(
                    model_ref=other_ref,
                    source_path=str(bound_source),
                    primary_format=ModelFormat.MLX,
                    capabilities=[ModelCapability.CHAT],
                ),
                messages=[ChatMessagePayload(role="user", content="Hello")],
            )

            with self.assertRaises(HTTPException) as raised:
                _build_chat_inference_request(
                    payload,
                    _request(home, RuntimeCapability.CHAT, bound_ref),
                )

        self.assertEqual(raised.exception.status_code, 400)
        self.assertIn("model-bound runtime", str(raised.exception.detail))


class RuntimeRouteMountTests(unittest.TestCase):
    def test_runtime_routes_expose_internal_aliases_and_legacy_paths(self) -> None:
        app = create_app(
            RuntimeServerConfig(
                host="127.0.0.1",
                port=0,
                capability=RuntimeCapability.EMBEDDING,
            )
        )

        self.assertTrue(_has_post_route(app, "/v1/embeddings"))
        self.assertTrue(_has_post_route(app, "/internal/v1/embeddings"))
        self.assertTrue(_has_post_route(app, "/v1/chat"))
        self.assertTrue(_has_post_route(app, "/internal/v1/chat"))


def _request(
    home: Path,
    capability: RuntimeCapability,
    model_ref: str,
) -> SimpleNamespace:
    return SimpleNamespace(
        app=SimpleNamespace(
            state=SimpleNamespace(
                runtime_config=RuntimeServerConfig(
                    host="127.0.0.1",
                    port=8799,
                    capability=capability,
                    server_ref="server-ref",
                    model_ref=model_ref,
                    home=home,
                )
            )
        )
    )


def _direct_request(home: Path, capability: RuntimeCapability) -> SimpleNamespace:
    return SimpleNamespace(
        app=SimpleNamespace(
            state=SimpleNamespace(
                runtime_config=RuntimeServerConfig(
                    host="127.0.0.1",
                    port=8799,
                    capability=capability,
                    server_ref="server-ref",
                    model_ref=None,
                    home=home,
                )
            )
        )
    )


def _write_model(
    home: Path,
    *,
    model_ref: str,
    format_: ModelFormat,
    capability: ModelCapability,
) -> Path:
    source_path = home / "models" / "store" / model_ref / "variants" / format_.value / "source"
    source_path.mkdir(parents=True)
    model_dir = home / "models" / "store" / model_ref
    (model_dir / "model.toml").write_text(
        "\n".join(
            [
                f'model_ref = "{model_ref}"',
                f'short_ref = "{model_ref[:12]}"',
                'source_kind = "huggingface"',
                'source_repo = "owner/model"',
                'source_revision = "revision"',
                f'primary_format = "{format_.value}"',
                f'detected_formats = ["{format_.value}"]',
                f'model_capabilities = ["{capability.value}"]',
                'model_capability_source = "explicit-user"',
                "file_count = 1",
                "total_bytes = 1",
                'imported_at = "2026-05-26T00:00:00Z"',
            ]
        ),
        encoding="utf-8",
    )
    (source_path.parent / "variant.toml").write_text(
        "\n".join(
            [
                f'format = "{format_.value}"',
                'status = "imported"',
                'import_method = "pull"',
                'relative_source_path = "source"',
            ]
        ),
        encoding="utf-8",
    )
    return source_path


def _has_post_route(app: object, path: str) -> bool:
    return any(
        getattr(route, "path", None) == path
        and "POST" in (getattr(route, "methods", None) or set())
        for route in getattr(app, "routes", [])
    )


if __name__ == "__main__":
    unittest.main()
