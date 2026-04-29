from __future__ import annotations

import unittest

from tentgent_daemon.datasets.provider import (
    DATASET_PROVIDER_SYSTEM_PROMPT,
    DatasetJsonlGenerationRequest,
    DatasetProviderCallRequest,
    DatasetProviderParseError,
    call_dataset_provider,
    generate_dataset_jsonl,
    parse_dataset_jsonl,
)
from tentgent_daemon.providers import ProviderChatRequest, ProviderChatResponse
from tentgent_daemon.runtime.chat import Message


CANONICAL_ROW = (
    '{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"},'
    '{"role":"assistant","content":"Hello"}],"metadata":{"task":"smoke"}}'
)


class FakeProviderClient:
    def __init__(self, text: str) -> None:
        self.text = text
        self.requests: list[ProviderChatRequest] = []

    def generate(self, request: ProviderChatRequest) -> ProviderChatResponse:
        self.requests.append(request)
        return ProviderChatResponse(text=self.text)


class DatasetProviderTests(unittest.TestCase):
    def test_call_dataset_provider_reuses_provider_chat_client(self) -> None:
        client = FakeProviderClient("provider text")

        response = call_dataset_provider(
            DatasetProviderCallRequest(
                provider="openai",
                model="gpt-4.1-mini",
                messages=(Message(role="user", content="Generate rows."),),
                max_tokens=128,
                temperature=0.1,
            ),
            client=client,
        )

        self.assertEqual(response.text, "provider text")
        self.assertEqual(client.requests[0].model, "gpt-4.1-mini")
        self.assertEqual(client.requests[0].max_tokens, 128)
        self.assertEqual(client.requests[0].temperature, 0.1)

    def test_call_dataset_provider_normalizes_claude_alias(self) -> None:
        client = FakeProviderClient("provider text")

        response = call_dataset_provider(
            DatasetProviderCallRequest(
                provider="claude",
                model="claude-3-5-sonnet-latest",
                messages=(Message(role="user", content="Generate rows."),),
            ),
            client=client,
        )

        self.assertEqual(response.provider, "anthropic")

    def test_generate_dataset_jsonl_builds_dataset_messages_and_parses_response(self) -> None:
        client = FakeProviderClient(CANONICAL_ROW)

        response = generate_dataset_jsonl(
            DatasetJsonlGenerationRequest(
                provider="anthropic",
                model="claude-3-5-sonnet-latest",
                prompt="Generate one Tentgent row.",
                max_tokens=256,
            ),
            client=client,
        )

        self.assertEqual(response.provider, "anthropic")
        self.assertEqual(response.model, "claude-3-5-sonnet-latest")
        self.assertEqual(len(response.records), 1)
        self.assertIn('"schema":"tentgent.chat.v1"', response.jsonl)
        request = client.requests[0]
        self.assertEqual(request.messages[0].role, "system")
        self.assertEqual(request.messages[0].content, DATASET_PROVIDER_SYSTEM_PROMPT)
        self.assertEqual(request.messages[1].content, "Generate one Tentgent row.")

    def test_parse_dataset_jsonl_accepts_markdown_fence(self) -> None:
        parsed = parse_dataset_jsonl(f"```jsonl\n{CANONICAL_ROW}\n```")

        self.assertEqual(len(parsed.records), 1)
        self.assertFalse(parsed.warnings)

    def test_parse_dataset_jsonl_ignores_outer_provider_prose(self) -> None:
        parsed = parse_dataset_jsonl(f"Here is the dataset:\n{CANONICAL_ROW}\nDone.")

        self.assertEqual(len(parsed.records), 1)
        self.assertEqual(parsed.warnings, ("ignored 2 non-JSON provider output line(s)",))

    def test_parse_dataset_jsonl_rejects_training_rows_without_final_assistant(self) -> None:
        with self.assertRaisesRegex(DatasetProviderParseError, "mask_prompt=true"):
            parse_dataset_jsonl(
                '{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"}]}'
            )

    def test_parse_dataset_jsonl_explains_top_level_completion_shape(self) -> None:
        with self.assertRaisesRegex(
            DatasetProviderParseError,
            "top-level `completion` is not Tentgent training JSONL",
        ):
            parse_dataset_jsonl(
                '{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"}],'
                '"completion":{"role":"assistant","content":"Hello"}}'
            )

    def test_parse_dataset_jsonl_repairs_extra_tool_result_brace(self) -> None:
        row = (
            '{"schema":"tentgent.chat.v1","messages":['
            '{"role":"user","content":"查詢天氣。"},'
            '{"role":"assistant","content":"","tool_calls":['
            '{"id":"call_1","name":"get_weather","arguments":{"location":"台北"}}]},'
            '{"role":"tool","tool_call_id":"call_1","name":"get_weather",'
            '"content":{"temperature":"22°C","condition":"多雲"}}},'
            '{"role":"assistant","content":"台北目前多雲，約22度。"}]}'
        )

        parsed = parse_dataset_jsonl(row)

        self.assertEqual(len(parsed.records), 1)
        self.assertEqual(
            parsed.warnings,
            ("repaired invalid JSON at provider output line 1",),
        )
        self.assertIn('"role":"tool"', parsed.jsonl)
        self.assertEqual(
            parsed.records[0]["messages"][-1],
            {"role": "assistant", "content": "台北目前多雲，約22度。"},
        )

    def test_generate_dataset_jsonl_attaches_raw_text_to_parse_errors(self) -> None:
        raw_text = '{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"}]}'
        client = FakeProviderClient(raw_text)

        with self.assertRaisesRegex(DatasetProviderParseError, "mask_prompt=true") as raised:
            generate_dataset_jsonl(
                DatasetJsonlGenerationRequest(
                    provider="openai",
                    model="gpt-4.1-mini",
                    prompt="Generate one row.",
                ),
                client=client,
            )

        self.assertEqual(raised.exception.raw_text, raw_text)

    def test_parse_dataset_jsonl_rejects_empty_provider_output(self) -> None:
        with self.assertRaisesRegex(DatasetProviderParseError, "provider output must not be empty"):
            parse_dataset_jsonl("   ")

    def test_parse_dataset_jsonl_accepts_legacy_eval_case_split(self) -> None:
        parsed = parse_dataset_jsonl(
            '{"case_id":"case-1","user_prompt":"Say hi.",'
            '"expected_behaviors":["answers briefly"]}',
            split="eval_cases",
        )

        self.assertEqual(len(parsed.records), 1)


if __name__ == "__main__":
    unittest.main()
