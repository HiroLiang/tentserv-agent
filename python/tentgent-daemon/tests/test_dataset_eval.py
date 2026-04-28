from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tentgent_daemon.datasets.eval import (
    DATASET_EVAL_REPORT_SCHEMA,
    build_dataset_eval_prompt,
    evaluate_dataset,
    load_dataset_sample,
    parse_provider_eval_report,
)
from tentgent_daemon.providers import ProviderChatRequest, ProviderChatResponse


CANONICAL_ROW = (
    '{"schema":"tentgent.chat.v1","id":"row-1","messages":['
    '{"role":"user","content":"你好"},'
    '{"role":"assistant","content":"你好，有什麼我可以幫忙的？咕嚕"}]}'
)


class FakeProviderClient:
    def __init__(self, text: str) -> None:
        self.text = text
        self.requests: list[ProviderChatRequest] = []

    def generate(self, request: ProviderChatRequest) -> ProviderChatResponse:
        self.requests.append(request)
        return ProviderChatResponse(text=self.text)


class DatasetEvalTests(unittest.TestCase):
    def test_load_dataset_sample_reads_train_records_and_local_issues(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "train.jsonl").write_text(
                CANONICAL_ROW
                + "\n"
                + '{"schema":"tentgent.chat.v1","messages":[{"role":"user","content":"Hi"}]}\n',
                encoding="utf-8",
            )

            sample = load_dataset_sample(root, split="train", max_records=5)

            self.assertEqual(sample.total_records, 2)
            self.assertEqual(len(sample.records), 2)
            self.assertEqual(sample.records[0].record_id, "row-1")
            self.assertEqual(len(sample.local_issues), 1)
            self.assertIn("final assistant", sample.local_issues[0]["message"])

    def test_build_dataset_eval_prompt_includes_criteria_and_report_shape(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "train.jsonl").write_text(CANONICAL_ROW + "\n", encoding="utf-8")
            sample = load_dataset_sample(root, split="train", max_records=1)

            prompt = build_dataset_eval_prompt(sample, criteria="Check gulu suffix.")

            self.assertIn("Check gulu suffix.", prompt)
            self.assertIn(DATASET_EVAL_REPORT_SCHEMA, prompt)
            self.assertIn("row-1", prompt)

    def test_parse_provider_eval_report_accepts_fenced_json(self) -> None:
        parsed = parse_provider_eval_report(
            'Here:\n```json\n{"summary":"ok","overall_score":92,"findings":[]}\n```'
        )

        self.assertEqual(parsed["summary"], "ok")
        self.assertEqual(parsed["overall_score"], 92)

    def test_evaluate_dataset_writes_reports_with_fake_provider(self) -> None:
        provider_report = {
            "schema": DATASET_EVAL_REPORT_SCHEMA,
            "summary": "Looks useful.",
            "overall_score": 88,
            "findings": [
                {
                    "severity": "info",
                    "category": "style",
                    "split": "train",
                    "line": 1,
                    "record_id": "row-1",
                    "message": "The style is consistent.",
                    "recommendation": "Keep similar variation.",
                }
            ],
            "recommendations": ["Add validation examples."],
        }
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            dataset = root / "dataset"
            output = root / "report"
            dataset.mkdir()
            (dataset / "train.jsonl").write_text(CANONICAL_ROW + "\n", encoding="utf-8")
            client = FakeProviderClient(json.dumps(provider_report))

            outcome = evaluate_dataset(
                provider="openai",
                model="gpt-4.1-mini",
                dataset_path=dataset,
                output_dir=output,
                criteria="Check style.",
                api_key="test-key",
                client=client,
            )

            self.assertEqual(outcome.reviewed_records, 1)
            self.assertEqual(outcome.finding_count, 1)
            self.assertEqual(outcome.overall_score, 88)
            self.assertTrue((output / "eval-report.json").is_file())
            self.assertTrue((output / "eval-report.md").is_file())
            self.assertTrue((output / "prompt.md").is_file())
            self.assertTrue((output / "provider-output.raw.txt").is_file())
            written = json.loads((output / "eval-report.json").read_text(encoding="utf-8"))
            self.assertEqual(written["summary"], "Looks useful.")
            self.assertEqual(client.requests[0].messages[0].role, "system")


if __name__ == "__main__":
    unittest.main()
