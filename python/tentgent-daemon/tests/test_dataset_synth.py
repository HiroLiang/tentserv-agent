from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tentgent_daemon.datasets.synth import (
    DATASET_SYNTH_MANIFEST_SCHEMA,
    DATASET_TEMPLATE_VERSION,
    build_dataset_generation_prompt,
    write_dataset_synth_package,
)


class DatasetSynthTests(unittest.TestCase):
    def test_build_prompt_includes_training_rules(self) -> None:
        prompt = build_dataset_generation_prompt(brief="Make two support examples.")

        self.assertIn(DATASET_TEMPLATE_VERSION, prompt)
        self.assertIn("Target split: `train`", prompt)
        self.assertIn("Each record must end with a final assistant answer.", prompt)
        self.assertIn("Make two support examples.", prompt)

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
            self.assertEqual(manifest["warnings"], ["ignored 1 non-JSON provider output line(s)"])

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


if __name__ == "__main__":
    unittest.main()
