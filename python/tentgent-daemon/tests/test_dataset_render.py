from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tentgent_daemon.datasets.render import render_training_dataset, validate_eval_cases
from tentgent_daemon.datasets.schema import render_backend_record


class DatasetRenderTests(unittest.TestCase):
    def test_render_training_dataset_accepts_legacy_eval_cases(self) -> None:
        with (
            tempfile.TemporaryDirectory() as source_tmp,
            tempfile.TemporaryDirectory() as output_tmp,
        ):
            source_dir = Path(source_tmp)
            output_dir = Path(output_tmp)
            write_jsonl(
                source_dir / "train.jsonl",
                [
                    {
                        "schema": "tentgent.chat.v1",
                        "messages": [
                            {"role": "user", "content": "Hi"},
                            {"role": "assistant", "content": "Hello"},
                        ],
                    }
                ],
            )
            write_jsonl(
                source_dir / "eval_cases.jsonl",
                [
                    {
                        "case_id": "tool_call_profile",
                        "input_language": "en",
                        "user_prompt": "Use tools to get Hiro's current role",
                        "tools_available": ["get_profile(field)"],
                        "expected_behaviors": ["emits function call to get_profile"],
                    }
                ],
            )

            summary = render_training_dataset(
                source_dir=source_dir,
                output_dir=output_dir,
                mask_prompt=False,
            )

            self.assertEqual(summary.eval_cases, 1)
            self.assertEqual(summary.splits[0].examples, 1)

    def test_validate_eval_cases_accepts_prompt_only_canonical_records(self) -> None:
        with tempfile.TemporaryDirectory() as source_tmp:
            path = Path(source_tmp) / "eval_cases.jsonl"
            write_jsonl(
                path,
                [
                    {
                        "schema": "tentgent.chat.v1",
                        "messages": [{"role": "user", "content": "Say hello."}],
                        "expected_behavior": {"answer_language": "en"},
                    }
                ],
            )

            self.assertEqual(validate_eval_cases(path), 1)

    def test_render_backend_record_accepts_function_tool_call_without_type(self) -> None:
        record = {
            "schema": "tentgent.chat.v1",
            "messages": [
                {"role": "user", "content": "Fetch profile."},
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "call_1",
                            "function": {
                                "name": "get_profile",
                                "arguments": "{\"field\":\"role\"}",
                            },
                        }
                    ],
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_1",
                    "name": "get_profile",
                    "content": {"role": "AI Engineer"},
                },
                {"role": "assistant", "content": "AI Engineer."},
            ],
        }

        rendered = render_backend_record(record, mask_prompt=False)

        self.assertIn("Assistant tool_call call_1 get_profile", rendered["text"])

    def test_mask_prompt_keeps_context_out_of_completion_target(self) -> None:
        record = {
            "schema": "tentgent.chat.v1",
            "messages": [
                {"role": "system", "content": "Return JSON only."},
                {"role": "user", "content": "Summarize Taipei in JSON."},
                {
                    "role": "assistant",
                    "content": '{"city":"Taipei","summary":"compact"}',
                },
            ],
        }

        rendered = render_backend_record(record, mask_prompt=True)

        self.assertIn("System: Return JSON only.", rendered["prompt"])
        self.assertIn("User: Summarize Taipei in JSON.", rendered["prompt"])
        self.assertTrue(rendered["prompt"].rstrip().endswith("Assistant:"))
        self.assertEqual(
            rendered["completion"],
            '{"city":"Taipei","summary":"compact"}',
        )
        self.assertNotIn("Assistant:", rendered["completion"])
        self.assertNotIn("User:", rendered["completion"])


def write_jsonl(path: Path, rows: list[dict[str, object]]) -> None:
    path.write_text(
        "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in rows),
        encoding="utf-8",
    )


if __name__ == "__main__":
    unittest.main()
