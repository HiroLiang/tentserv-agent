from __future__ import annotations

import unittest
from typing import Any

from tentgent_daemon.providers import (
    ANTHROPIC_VERSION,
    DEFAULT_ANTHROPIC_MAX_TOKENS,
    ANTHROPIC_MESSAGES_URL,
    OPENAI_CHAT_COMPLETIONS_URL,
    AnthropicChatClient,
    OpenAIChatClient,
    ProviderChatRequest,
    ProviderRequestError,
    ProviderResponseError,
    create_provider_chat_client,
)
from tentgent_daemon.runtime.chat import Message


class FakeTransport:
    def __init__(self, status: int, body: dict[str, Any]) -> None:
        self.status = status
        self.body = body
        self.calls: list[tuple[str, dict[str, str], dict[str, Any]]] = []

    def post_json(
        self,
        url: str,
        headers: dict[str, str],
        payload: dict[str, Any],
    ) -> tuple[int, dict[str, Any]]:
        self.calls.append((url, headers, payload))
        return self.status, self.body


class ProviderChatTests(unittest.TestCase):
    def test_openai_maps_request_and_parses_text(self) -> None:
        transport = FakeTransport(
            200,
            {"choices": [{"message": {"content": "Hello from OpenAI"}}]},
        )
        client = OpenAIChatClient("sk-test", transport=transport)

        response = client.generate(
            ProviderChatRequest(
                model="gpt-4.1-mini",
                messages=(
                    Message(role="system", content="Be concise."),
                    Message(role="user", content="Hi"),
                ),
                max_tokens=64,
                temperature=0.2,
            )
        )

        self.assertEqual(response.text, "Hello from OpenAI")
        url, headers, payload = transport.calls[0]
        self.assertEqual(url, OPENAI_CHAT_COMPLETIONS_URL)
        self.assertEqual(headers["Authorization"], "Bearer sk-test")
        self.assertEqual(payload["model"], "gpt-4.1-mini")
        self.assertEqual(payload["max_tokens"], 64)
        self.assertEqual(payload["temperature"], 0.2)
        self.assertEqual(
            payload["messages"],
            [
                {"role": "system", "content": "Be concise."},
                {"role": "user", "content": "Hi"},
            ],
        )

    def test_anthropic_moves_system_to_top_level_and_parses_text(self) -> None:
        transport = FakeTransport(
            200,
            {"content": [{"type": "text", "text": "Hello from Anthropic"}]},
        )
        client = AnthropicChatClient("sk-ant-test", transport=transport)

        response = client.generate(
            ProviderChatRequest(
                model="claude-3-5-sonnet-latest",
                messages=(
                    Message(role="system", content="Be helpful."),
                    Message(role="system", content="Prefer short answers."),
                    Message(role="user", content="Hi"),
                ),
                temperature=0.1,
            )
        )

        self.assertEqual(response.text, "Hello from Anthropic")
        url, headers, payload = transport.calls[0]
        self.assertEqual(url, ANTHROPIC_MESSAGES_URL)
        self.assertEqual(headers["x-api-key"], "sk-ant-test")
        self.assertEqual(headers["anthropic-version"], ANTHROPIC_VERSION)
        self.assertEqual(payload["model"], "claude-3-5-sonnet-latest")
        self.assertEqual(payload["max_tokens"], DEFAULT_ANTHROPIC_MAX_TOKENS)
        self.assertEqual(payload["temperature"], 0.1)
        self.assertEqual(payload["system"], "Be helpful.\n\nPrefer short answers.")
        self.assertEqual(payload["messages"], [{"role": "user", "content": "Hi"}])

    def test_anthropic_requires_non_system_message(self) -> None:
        client = AnthropicChatClient("sk-ant-test", transport=FakeTransport(200, {}))

        with self.assertRaisesRegex(
            ProviderRequestError,
            "at least one user or assistant message",
        ):
            client.generate(
                ProviderChatRequest(
                    model="claude-3-5-sonnet-latest",
                    messages=(Message(role="system", content="Only system"),),
                )
            )

    def test_provider_error_does_not_include_secret(self) -> None:
        transport = FakeTransport(
            401,
            {"error": {"message": "bad key"}},
        )
        client = OpenAIChatClient("super-secret-key", transport=transport)

        with self.assertRaises(ProviderResponseError) as captured:
            client.generate(
                ProviderChatRequest(
                    model="gpt-4.1-mini",
                    messages=(Message(role="user", content="Hi"),),
                )
            )

        message = str(captured.exception)
        self.assertIn("OpenAI returned HTTP 401", message)
        self.assertIn("bad key", message)
        self.assertNotIn("super-secret-key", message)

    def test_factory_accepts_claude_alias(self) -> None:
        client = create_provider_chat_client(
            "claude",
            "sk-ant-test",
            transport=FakeTransport(200, {"content": [{"type": "text", "text": "ok"}]}),
        )

        response = client.generate(
            ProviderChatRequest(
                model="claude-3-5-sonnet-latest",
                messages=(Message(role="user", content="Hi"),),
            )
        )

        self.assertEqual(response.text, "ok")


if __name__ == "__main__":
    unittest.main()
