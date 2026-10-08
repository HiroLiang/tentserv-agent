"""Offline regressions for current-checkout, non-editable release validation."""

import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "release_python_source", Path(__file__).with_name("check-release-python-source.py")
)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class ReleaseSourceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="tentgent-source-guard-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.source = root / "source"
        self.site_packages = root / "env/site-packages"
        self.installed = self.site_packages / "tentgent/runtime"
        for directory in (self.source, self.installed):
            directory.mkdir(parents=True)
            (directory / "guard.py").write_text("version = '0.1.0'\nfixed = True\n")

    def verify(self):
        return checker.verify_source_tree(
            self.source, self.installed, self.site_packages
        )

    def test_matching_installed_tree_passes_and_ignores_bytecode(self):
        cache = self.installed / "__pycache__"
        cache.mkdir()
        (cache / "guard.pyc").write_bytes(b"bytecode")
        self.assertEqual(self.verify(), 1)

    def test_same_version_stale_source_is_rejected(self):
        (self.installed / "guard.py").write_text("version = '0.1.0'\nfixed = False\n")
        with self.assertRaisesRegex(RuntimeError, "content differs.*guard.py"):
            self.verify()

    def test_missing_installed_module_is_rejected(self):
        (self.source / "new_guard.py").write_text("fixed = True\n")
        with self.assertRaisesRegex(RuntimeError, "missing=.*new_guard.py"):
            self.verify()

    def test_removed_module_left_in_wheel_is_rejected(self):
        (self.installed / "removed_guard.py").write_text("fixed = False\n")
        with self.assertRaisesRegex(RuntimeError, "extra=.*removed_guard.py"):
            self.verify()

    def test_editable_or_source_fallback_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "installed site-packages"):
            checker.verify_source_tree(self.source, self.source, self.site_packages)

    def test_empty_source_tree_is_not_success(self):
        empty = self.source / "empty"
        empty.mkdir()
        with self.assertRaisesRegex(RuntimeError, "source tree is empty"):
            checker.verify_source_tree(empty, self.installed, self.site_packages)


if __name__ == "__main__":
    unittest.main()
