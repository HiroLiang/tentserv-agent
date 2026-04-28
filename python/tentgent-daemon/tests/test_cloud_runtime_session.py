from __future__ import annotations

import json
import unittest
from pathlib import Path
from unittest.mock import patch

from tentgent_daemon.providers import ProviderChatResponse
from tentgent_daemon.runtime.chat import Message
from tentgent_daemon.server.chat_api import handle_chat_request
from tentgent_daemon.server.config import ServerConfig
from tentgent_daemon.server.health import build_health_payload
from tentgent_daemon.server.session import ChatRequestPayload, RuntimeSession


class FakeProviderClient:
    def __init__(self) -> None:
        self.requests = []

    def generate(self, request):
        self.requests.append(request)
        return ProviderChatResponse(text="hello from cloud")


class CloudRuntimeSessionTests(unittest.TestCase):
    def test_cloud_session_generates_with_provider_client(self) -> None:
        client = FakeProviderClient()
        with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}), patch(
            "tentgent_daemon.server.session.create_provider_chat_client",
            return_value=client,
        ) as create_client:
            session = RuntimeSession(cloud_config())

        create_client.assert_called_once_with("openai", "sk-test")

        text = session.generate(
            ChatRequestPayload(
                messages=(Message(role="user", content="Hi"),),
                max_tokens=32,
                temperature=0.3,
                adapter_ref=None,
                stream=False,
            )
        )

        self.assertEqual(text, "hello from cloud")
        self.assertEqual(len(client.requests), 1)
        request = client.requests[0]
        self.assertEqual(request.model, "gpt-4.1-mini")
        self.assertEqual(request.messages, (Message(role="user", content="Hi"),))
        self.assertEqual(request.max_tokens, 32)
        self.assertEqual(request.temperature, 0.3)

        snapshot = session.snapshot()
        self.assertFalse(snapshot.loaded)
        self.assertEqual(snapshot.startup_mode, "cloud_proxy")
        self.assertEqual(snapshot.idle_policy, "stateless_proxy")
        self.assertIsNotNone(snapshot.last_activity_at)

    def test_cloud_chat_request_uses_provider_client(self) -> None:
        client = FakeProviderClient()
        with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}), patch(
            "tentgent_daemon.server.session.create_provider_chat_client",
            return_value=client,
        ):
            session = RuntimeSession(cloud_config())

        status, payload = handle_chat_request(
            json.dumps(
                {
                    "messages": [{"role": "user", "content": "Hello"}],
                    "max_tokens": 16,
                    "temperature": 0.0,
                }
            ).encode("utf-8"),
            session,
        )

        self.assertEqual(status.value, 200)
        self.assertEqual(payload, {"text": "hello from cloud", "stream": False})
        self.assertEqual(client.requests[0].model, "gpt-4.1-mini")

    def test_cloud_runtime_rejects_adapter_ref(self) -> None:
        with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}), patch(
            "tentgent_daemon.server.session.create_provider_chat_client",
            return_value=FakeProviderClient(),
        ):
            session = RuntimeSession(cloud_config())

        status, payload = handle_chat_request(
            json.dumps(
                {
                    "messages": [{"role": "user", "content": "Hello"}],
                    "adapter_ref": "adapter-ref",
                }
            ).encode("utf-8"),
            session,
        )

        self.assertEqual(status.value, 501)
        self.assertEqual(payload["error"], "adapter_execution_not_implemented")

    def test_cloud_health_reports_provider_runtime(self) -> None:
        with patch.dict("os.environ", {"OPENAI_API_KEY": "sk-test"}), patch(
            "tentgent_daemon.server.session.create_provider_chat_client",
            return_value=FakeProviderClient(),
        ):
            session = RuntimeSession(cloud_config())

        payload = build_health_payload(cloud_config(), session)

        self.assertEqual(payload["runtime_kind"], "cloud")
        self.assertEqual(payload["provider"], "openai")
        self.assertEqual(payload["provider_model"], "gpt-4.1-mini")
        self.assertEqual(payload["startup_mode"], "cloud_proxy")
        self.assertEqual(payload["idle_policy"], "stateless_proxy")
        self.assertFalse(payload["model_loaded"])


def cloud_config() -> ServerConfig:
    return ServerConfig(
        server_ref="server-ref",
        runtime_kind="cloud",
        model_ref=None,
        provider="openai",
        provider_model="gpt-4.1-mini",
        host="127.0.0.1",
        port=8780,
        home=Path("/tmp/tentgent-cloud-session-test"),
        lazy_load=False,
        idle_seconds=None,
    )


if __name__ == "__main__":
    unittest.main()
