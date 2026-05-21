from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
import unittest
from unittest.mock import patch

from tentgent_daemon.runtime.adapters import StoredAdapterRecord
from tentgent_daemon.runtime.chat import Message
from tentgent_daemon.runtime.records import StoredModelRecord
from tentgent_daemon.runtime.rerank import RerankResult, RerankScore
from tentgent_daemon.runtime.router import BackendKind
from tentgent_daemon.server.config import ServerConfig
from tentgent_daemon.server.session import ChatRequestPayload, RuntimeSession


class FakeStreamingBackend:
    def __init__(self) -> None:
        self.loaded_records: list[StoredModelRecord] = []
        self.selected_adapters: list[StoredAdapterRecord | None] = []
        self.stream_requests: list[object] = []

    def load(self, record: StoredModelRecord) -> None:
        self.loaded_records.append(record)

    def release(self) -> None:
        return

    def select_adapter(self, adapter: StoredAdapterRecord | None) -> None:
        self.selected_adapters.append(adapter)

    def stream_generate(self, request: object):
        self.stream_requests.append(request)
        yield "你"
        yield "好"


class FakeEmbeddingBackend:
    def __init__(self) -> None:
        self.loaded_records: list[StoredModelRecord] = []
        self.requests: list[object] = []

    def load(self, record: StoredModelRecord) -> None:
        self.loaded_records.append(record)

    def release(self) -> None:
        return

    def embed(self, request: object):
        self.requests.append(request)
        return type("EmbeddingResult", (), {"vectors": [[0.1, 0.2], [0.3, 0.4]]})()


class FakeRerankBackend:
    def __init__(self) -> None:
        self.loaded_records: list[StoredModelRecord] = []
        self.requests: list[object] = []

    def load(self, record: StoredModelRecord) -> None:
        self.loaded_records.append(record)

    def release(self) -> None:
        return

    def rerank(self, request: object) -> RerankResult:
        self.requests.append(request)
        return RerankResult(data=(RerankScore(index=1, score=0.9),))


class RuntimeSessionStreamingTests(unittest.TestCase):
    def test_local_stream_generate_loads_backend_and_marks_activity(self) -> None:
        backend = FakeStreamingBackend()
        with patched_local_runtime(backend):
            session = RuntimeSession(local_config())
            chunks = list(
                session.stream_generate(
                    ChatRequestPayload(
                        messages=(Message(role="user", content="Hi"),),
                        max_tokens=8,
                        temperature=0.0,
                        adapter_ref=None,
                        stream=True,
                    )
                )
            )

        self.assertEqual(chunks, ["你", "好"])
        self.assertEqual(len(backend.loaded_records), 1)
        self.assertEqual(backend.selected_adapters, [None, None])
        self.assertEqual(len(backend.stream_requests), 1)
        snapshot = session.snapshot()
        self.assertTrue(snapshot.loaded)
        self.assertEqual(snapshot.startup_mode, "eager")
        self.assertIsNotNone(snapshot.last_activity_at)

    def test_local_stream_generate_selects_adapter_before_streaming(self) -> None:
        backend = FakeStreamingBackend()
        adapter = fake_adapter()
        with patched_local_runtime(backend, adapter=adapter):
            session = RuntimeSession(local_config())
            chunks = list(
                session.stream_generate(
                    ChatRequestPayload(
                        messages=(Message(role="user", content="Hi"),),
                        max_tokens=8,
                        temperature=0.0,
                        adapter_ref=adapter.short_ref,
                        stream=True,
                    )
                )
            )

        self.assertEqual(chunks, ["你", "好"])
        self.assertEqual(len(backend.loaded_records), 1)
        self.assertEqual(backend.selected_adapters, [adapter, adapter])
        self.assertEqual(backend.stream_requests[0].adapter_ref, adapter.adapter_ref)

    def test_local_embedding_request_loads_embedding_backend(self) -> None:
        backend = FakeEmbeddingBackend()
        with patched_local_embedding_runtime(backend):
            session = RuntimeSession(embedding_config())
            vectors = session.embed(("first", "second"))

        self.assertEqual(vectors, [[0.1, 0.2], [0.3, 0.4]])
        self.assertEqual(len(backend.loaded_records), 1)
        self.assertEqual(len(backend.requests), 1)
        self.assertEqual(backend.requests[0].inputs, ("first", "second"))
        snapshot = session.snapshot()
        self.assertTrue(snapshot.loaded)
        self.assertEqual(snapshot.startup_mode, "eager")

    def test_local_rerank_request_loads_rerank_backend(self) -> None:
        backend = FakeRerankBackend()
        with patched_local_rerank_runtime(backend):
            session = RuntimeSession(rerank_config())
            results = session.rerank("query", ("first", "second"), 1)

        self.assertEqual(results, (RerankScore(index=1, score=0.9),))
        self.assertEqual(len(backend.loaded_records), 1)
        self.assertEqual(len(backend.requests), 1)
        self.assertEqual(backend.requests[0].query, "query")
        self.assertEqual(backend.requests[0].documents, ("first", "second"))
        self.assertEqual(backend.requests[0].top_n, 1)
        snapshot = session.snapshot()
        self.assertTrue(snapshot.loaded)
        self.assertEqual(snapshot.startup_mode, "eager")


@contextmanager
def patched_local_runtime(
    backend: FakeStreamingBackend,
    adapter: StoredAdapterRecord | None = None,
) -> Iterator[None]:
    with patch(
        "tentgent_daemon.server.session.load_model_record",
        return_value=fake_record(),
    ), patch(
        "tentgent_daemon.server.session.resolve_backend",
        return_value=BackendKind.MLX,
    ), patch(
        "tentgent_daemon.server.session.create_backend",
        return_value=backend,
    ), patch(
        "tentgent_daemon.server.session.load_adapter_record",
        return_value=adapter,
    ):
        yield


@contextmanager
def patched_local_embedding_runtime(
    backend: FakeEmbeddingBackend,
) -> Iterator[None]:
    with patch(
        "tentgent_daemon.server.session.load_model_record",
        return_value=fake_embedding_record(),
    ), patch(
        "tentgent_daemon.server.session.resolve_embedding_backend",
        return_value=BackendKind.TRANSFORMERS_PEFT,
    ), patch(
        "tentgent_daemon.server.session.create_embedding_backend",
        return_value=backend,
    ):
        yield


@contextmanager
def patched_local_rerank_runtime(
    backend: FakeRerankBackend,
) -> Iterator[None]:
    with patch(
        "tentgent_daemon.server.session.load_model_record",
        return_value=fake_rerank_record(),
    ), patch(
        "tentgent_daemon.server.session.resolve_rerank_backend",
        return_value=BackendKind.TRANSFORMERS_PEFT,
    ), patch(
        "tentgent_daemon.server.session.create_rerank_backend",
        return_value=backend,
    ):
        yield


def local_config() -> ServerConfig:
    return ServerConfig(
        server_ref="server-ref",
        runtime_kind="local",
        capability="chat",
        model_ref="model-ref",
        provider=None,
        provider_model=None,
        host="127.0.0.1",
        port=8780,
        home=Path("/tmp/tentgent-local-session-stream-test"),
        lazy_load=False,
        idle_seconds=None,
    )


def embedding_config() -> ServerConfig:
    return ServerConfig(
        server_ref="server-ref",
        runtime_kind="local",
        capability="embedding",
        model_ref="model-ref",
        provider=None,
        provider_model=None,
        host="127.0.0.1",
        port=8780,
        home=Path("/tmp/tentgent-local-session-stream-test"),
        lazy_load=False,
        idle_seconds=None,
    )


def rerank_config() -> ServerConfig:
    return ServerConfig(
        server_ref="server-ref",
        runtime_kind="local",
        capability="rerank",
        model_ref="model-ref",
        provider=None,
        provider_model=None,
        host="127.0.0.1",
        port=8780,
        home=Path("/tmp/tentgent-local-session-stream-test"),
        lazy_load=False,
        idle_seconds=None,
    )


def fake_record() -> StoredModelRecord:
    root = Path("/tmp/tentgent-local-session-stream-test/model")
    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="local",
        source_repo=None,
        source_revision=None,
        source_path=None,
        primary_format="mlx",
        detected_formats=("mlx",),
        mlx_runtime_family=None,
        model_capabilities=("chat",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-04-29T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / "mlx" / "source",
    )


def fake_embedding_record() -> StoredModelRecord:
    root = Path("/tmp/tentgent-local-session-stream-test/model")
    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="local",
        source_repo=None,
        source_revision=None,
        source_path=None,
        primary_format="safetensors",
        detected_formats=("safetensors",),
        mlx_runtime_family=None,
        model_capabilities=("embedding",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-04-29T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / "safetensors" / "source",
    )


def fake_rerank_record() -> StoredModelRecord:
    root = Path("/tmp/tentgent-local-session-stream-test/model")
    return StoredModelRecord(
        model_ref="model-ref",
        short_ref="model",
        source_kind="local",
        source_repo=None,
        source_revision=None,
        source_path=None,
        primary_format="safetensors",
        detected_formats=("safetensors",),
        mlx_runtime_family=None,
        model_capabilities=("rerank",),
        file_count=1,
        total_bytes=1,
        imported_at="2026-04-29T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        variant_source_path=root / "variants" / "safetensors" / "source",
    )


def fake_adapter() -> StoredAdapterRecord:
    root = Path("/tmp/tentgent-local-session-stream-test/adapter")
    return StoredAdapterRecord(
        adapter_ref="adapter-ref",
        short_ref="adapter",
        adapter_format="mlx",
        adapter_type="lora",
        target_capability="chat",
        base_model_ref="model-ref",
        base_model_source_repo=None,
        base_model_source_revision=None,
        model_family=None,
        backend_support=("mlx",),
        weight_file=None,
        trigger_words=(),
        recommended_scale=None,
        source_kind="train-run",
        source_repo=None,
        source_revision=None,
        source_path=None,
        file_count=1,
        total_bytes=1,
        imported_at="2026-04-29T00:00:00Z",
        store_path=root,
        manifest_path=root / "manifest.json",
        source_dir=root / "source",
    )


if __name__ == "__main__":
    unittest.main()
