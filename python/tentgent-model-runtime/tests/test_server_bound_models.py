from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from tentgent.runtime.backends.chat import ChatModelKind
from tentgent.runtime.backends.embedding import EmbeddingModelKind
from tentgent.runtime.backends.records import ModelCapability, ModelFormat
from tentgent.runtime.backends.rerank import RerankModelKind
from tentgent.runtime.server.lifecycle import RuntimeCapability, RuntimeServerConfig
from tentgent.runtime.server.managed_models import load_managed_model_record
from tentgent.runtime.server.routes.chat import (
    ChatMessagePayload,
    ChatPayload,
    _build_chat_inference_request,
)
from tentgent.runtime.server.routes.embedding import (
    EmbeddingPayload,
    _build_embedding_inference_request,
)
from tentgent.runtime.server.routes.rerank import (
    RerankPayload,
    _build_rerank_inference_request,
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


if __name__ == "__main__":
    unittest.main()
