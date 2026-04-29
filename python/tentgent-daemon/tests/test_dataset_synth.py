from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tentgent_daemon.datasets.synth import (
    DATASET_SYNTH_MANIFEST_SCHEMA,
    DATASET_TEMPLATE_VERSION,
    DatasetSynthSplitInput,
    build_dataset_generation_prompt,
    write_dataset_synth_package_multi,
    write_dataset_synth_package,
)
from tentgent_daemon.cli.dataset_synth import (
    build_retry_generation_prompt,
    is_retryable_generation_error,
    write_failure_debug,
)
from tentgent_daemon.providers import ProviderRequestError, ProviderResponseError


class DatasetSynthTests(unittest.TestCase):
    def test_build_prompt_includes_training_rules(self) -> None:
        prompt = build_dataset_generation_prompt(brief="Make two support examples.")

        self.assertIn(DATASET_TEMPLATE_VERSION, prompt)
        self.assertIn("Target split: `train`", prompt)
        self.assertIn("Each record must end with a final assistant answer.", prompt)
        self.assertIn("Do not use top-level `completion`", prompt)
        self.assertIn('"messages":[{"role":"user"', prompt)
        self.assertIn('"role":"assistant","content"', prompt)
        self.assertIn("Make two support examples.", prompt)

    def test_build_prompt_includes_requested_record_count(self) -> None:
        prompt = build_dataset_generation_prompt(
            brief="Make support examples.",
            split="valid",
            record_count=3,
        )

        self.assertIn("Target split: `valid`", prompt)
        self.assertIn("Requested records: `3`", prompt)
        self.assertIn("Generate exactly 3 JSONL record(s).", prompt)

    def test_build_prompt_includes_eval_case_shape(self) -> None:
        prompt = build_dataset_generation_prompt(
            brief="Make eval cases.",
            split="eval_cases",
            record_count=2,
        )

        self.assertIn("Target split: `eval_cases`", prompt)
        self.assertIn('"expected_behavior"', prompt)
        self.assertIn('"metadata":{"split":"eval_cases"', prompt)

    def test_build_retry_prompt_restates_canonical_training_shape(self) -> None:
        prompt = build_retry_generation_prompt(
            "Generate Tentgent dataset JSONL.",
            split="valid",
            error="invalid JSON at provider output line 3",
        )

        self.assertIn("invalid JSON at provider output line 3", prompt)
        self.assertIn("Generate a fresh complete `valid` JSONL output", prompt)
        self.assertIn("final assistant answer inside `messages`", prompt)
        self.assertIn("Do not use top-level `completion`", prompt)

    def test_generation_retry_policy_skips_request_and_non_transient_4xx(self) -> None:
        self.assertFalse(is_retryable_generation_error(ProviderRequestError("bad request")))
        self.assertFalse(
            is_retryable_generation_error(
                ProviderResponseError("OpenAI returned HTTP 404: model not found")
            )
        )
        self.assertTrue(
            is_retryable_generation_error(
                ProviderResponseError("OpenAI returned HTTP 429: rate limited")
            )
        )

    def test_write_dataset_synth_package_writes_split_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "generated"
            outcome = write_dataset_synth_package(
                output_dir=output_dir,
                provider="openai",
                model="gpt-4.1-mini",
                split="train",
                jsonl='{"messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}\n',
                record_count=1,
                prompt_source_kind="brief",
                prompt_source_text="Make one row.",
                prompt_source_path=None,
                warnings=("ignored 1 non-JSON provider output line(s)",),
                max_tokens=512,
                temperature=0.0,
            )

            self.assertEqual(outcome.record_count, 1)
            self.assertTrue((output_dir / "train.jsonl").is_file())
            manifest = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["schema"], DATASET_SYNTH_MANIFEST_SCHEMA)
            self.assertEqual(manifest["generated_by"]["provider"], "openai")
            self.assertEqual(manifest["record_count"], 1)
            self.assertEqual(manifest["splits"]["train"]["path"], "train.jsonl")
            self.assertEqual(manifest["warnings"], ["ignored 1 non-JSON provider output line(s)"])

    def test_write_dataset_synth_package_multi_writes_splits_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "generated"
            outcome = write_dataset_synth_package_multi(
                output_dir=output_dir,
                provider="openai",
                model="gpt-4.1-mini",
                split_inputs=(
                    DatasetSynthSplitInput(
                        split="train",
                        jsonl='{"messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"}]}\n',
                        record_count=1,
                        warnings=(),
                    ),
                    DatasetSynthSplitInput(
                        split="valid",
                        jsonl='{"messages":[{"role":"user","content":"Bye"},{"role":"assistant","content":"Goodbye"}]}\n',
                        record_count=1,
                        warnings=("valid warning",),
                    ),
                ),
                prompt_source_kind="brief",
                prompt_source_text="Make rows.",
                prompt_source_path=None,
                max_tokens=512,
                temperature=0.0,
            )

            self.assertEqual(outcome.record_count, 2)
            self.assertTrue((output_dir / "train.jsonl").is_file())
            self.assertTrue((output_dir / "valid.jsonl").is_file())
            manifest = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["record_count"], 2)
            self.assertEqual(manifest["splits"]["train"]["record_count"], 1)
            self.assertEqual(manifest["splits"]["valid"]["warnings"], ["valid warning"])
            self.assertEqual(manifest["warnings"], ["valid warning"])

    def test_write_dataset_synth_package_refuses_non_empty_output_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp)
            (output_dir / "existing.txt").write_text("occupied", encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "output directory must be empty"):
                write_dataset_synth_package(
                    output_dir=output_dir,
                    provider="openai",
                    model="gpt-4.1-mini",
                    split="train",
                    jsonl="{}\n",
                    record_count=1,
                    prompt_source_kind="brief",
                    prompt_source_text="Make one row.",
                    prompt_source_path=None,
                    warnings=(),
                    max_tokens=None,
                    temperature=0.0,
                )

    def test_write_failure_debug_writes_prompt_raw_output_and_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "failed"
            debug_dir = write_failure_debug(
                output_dir,
                split=None,
                prompt="Generate rows.",
                raw_text='{"bad":true}',
                error="provider row failed",
            )

            self.assertEqual(debug_dir, output_dir / "_debug")
            self.assertEqual(
                (debug_dir / "prompt.md").read_text(encoding="utf-8"),
                "Generate rows.",
            )
            self.assertEqual(
                (debug_dir / "provider-output.raw.txt").read_text(encoding="utf-8"),
                '{"bad":true}',
            )
            self.assertEqual(
                (debug_dir / "error.txt").read_text(encoding="utf-8"),
                "provider row failed\n",
            )

    def test_write_failure_debug_refuses_non_empty_output_dir(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp)
            (output_dir / "existing.txt").write_text("occupied", encoding="utf-8")

            self.assertIsNone(
                write_failure_debug(
                    output_dir,
                    split=None,
                    prompt="Generate rows.",
                    raw_text='{"bad":true}',
                    error="provider row failed",
                )
            )

    def test_write_failure_debug_allows_partial_generated_splits(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp)
            (output_dir / "train.jsonl").write_text("{}", encoding="utf-8")

            debug_dir = write_failure_debug(
                output_dir,
                split="valid",
                prompt="Generate validation rows.",
                raw_text=None,
                error="timed out",
            )

            self.assertEqual(debug_dir, output_dir / "_debug" / "valid")
            self.assertEqual(
                (debug_dir / "prompt.md").read_text(encoding="utf-8"),
                "Generate validation rows.",
            )
            self.assertEqual(
                (debug_dir / "error.txt").read_text(encoding="utf-8"),
                "timed out\n",
            )


if __name__ == "__main__":
    unittest.main()
