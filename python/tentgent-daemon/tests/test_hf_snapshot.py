from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from tentgent_daemon.tools.hf_snapshot import build_hf_metadata, write_result_file


class HfSnapshotTests(unittest.TestCase):
    def test_build_hf_metadata_collects_registry_and_snapshot_hints(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            local_dir = Path(temp_dir)
            (local_dir / "config.json").write_text(
                '{"architectures":["BertForSequenceClassification"]}',
                encoding="utf-8",
            )
            (local_dir / "tokenizer_config.json").write_text(
                '{"chat_template":"{{ messages }}"}',
                encoding="utf-8",
            )
            (local_dir / "sentence_bert_config.json").write_text("{}", encoding="utf-8")
            info = SimpleNamespace(
                pipeline_tag="sentence-similarity",
                tags=["sentence-transformers", "feature-extraction"],
                library_name="sentence-transformers",
            )

            metadata = build_hf_metadata(info, local_dir)

            self.assertEqual(metadata["pipeline_tag"], "sentence-similarity")
            self.assertEqual(metadata["tags"], ["sentence-transformers", "feature-extraction"])
            self.assertEqual(metadata["library_name"], "sentence-transformers")
            self.assertEqual(
                metadata["config_architectures"], ["BertForSequenceClassification"]
            )
            self.assertTrue(metadata["tokenizer_chat_template"])
            self.assertTrue(metadata["sentence_bert_config"])

    def test_write_result_file_includes_compact_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            result_path = temp_path / "result.json"
            local_dir = temp_path / "snapshot"
            metadata = {
                "pipeline_tag": None,
                "tags": [],
                "library_name": None,
                "config_architectures": [],
                "tokenizer_chat_template": False,
                "sentence_bert_config": False,
            }

            write_result_file(
                result_path,
                repo_id="org/model",
                resolved_revision="sha",
                local_dir=local_dir,
                metadata=metadata,
            )

            payload = json.loads(result_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["repo_id"], "org/model")
            self.assertEqual(payload["resolved_revision"], "sha")
            self.assertEqual(payload["local_dir"], str(local_dir))
            self.assertEqual(payload["metadata"], metadata)


if __name__ == "__main__":
    unittest.main()
