"""Offline unit tests for native artifact-smoke arguments and isolation."""

import argparse
import hashlib
import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "installed_release", Path(__file__).with_name("test-installed-release.py")
)
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class InstalledReleaseTests(unittest.TestCase):
    def test_stable_and_rc_version_arguments(self):
        for raw, version in (("v1.2.3", "1.2.3"), ("1.2.3-rc.1", "1.2.3-rc.1")):
            with self.subTest(raw=raw):
                args = smoke.parse_args(
                    [
                        "--archive",
                        "release.tar.gz",
                        "--checksums",
                        "checksums.txt",
                        "--target",
                        "aarch64-apple-darwin",
                        "--version",
                        raw,
                    ]
                )
                self.assertEqual(args.version, version)

    def test_invalid_version_is_rejected(self):
        with self.assertRaises(SystemExit) as caught:
            smoke.parse_args(
                [
                    "--archive",
                    "release.tar.gz",
                    "--checksums",
                    "checksums.txt",
                    "--target",
                    "aarch64-apple-darwin",
                    "--version",
                    "1.2.3/../../path",
                ]
            )
        self.assertEqual(caught.exception.code, 2)

    def test_overrides_are_removed_without_repurposing_os_home(self):
        root = Path("/isolated")
        env = smoke.isolated_environment(
            root,
            {
                "HOME": "/original/home",
                "PATH": "/bin",
                "PYTHONPATH": "/repo",
                "TENTGENT_MODELS_DIR": "/user-models",
                "UV_PROJECT_ENVIRONMENT": "/repo/.venv",
                "UV_PYTHON_INSTALL_DIR": "/global/python",
                "VIRTUAL_ENV": "/repo/.venv",
                "CONDA_PREFIX": "/user/conda",
                "pythonhome": "/python",
            },
        )
        self.assertEqual(env["HOME"], "/original/home")
        self.assertEqual(env["TENTGENT_HOME"], str(root / "home"))
        self.assertEqual(env["UV_PYTHON_INSTALL_DIR"], str(root / "managed-python"))
        for key in (
            "PYTHONPATH",
            "TENTGENT_MODELS_DIR",
            "UV_PROJECT_ENVIRONMENT",
            "VIRTUAL_ENV",
            "CONDA_PREFIX",
            "pythonhome",
        ):
            self.assertNotIn(key, env)

    def test_installers_bootstrap_python_and_only_skip_doctor(self):
        for target, skip in (
            ("aarch64-apple-darwin", "--skip-doctor"),
            ("x86_64-pc-windows-msvc", "-SkipDoctor"),
        ):
            with self.subTest(target=target):
                command = smoke.installer_command(
                    argparse.Namespace(
                        target=target,
                        archive=Path("archive"),
                        checksums=Path("sums"),
                        version="1.2.3",
                    ),
                    Path("prefix"),
                )
                self.assertIn(skip, command)
                self.assertNotIn("--skip-python-bootstrap", command)
                self.assertNotIn("-SkipPythonBootstrap", command)

    def test_checksum_accepts_exact_entry_and_rejects_mismatch_or_duplicates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, sums = root / "release.tar.gz", root / "checksums.txt"
            archive.write_bytes(b"fixture")
            digest = hashlib.sha256(b"fixture").hexdigest()
            entry = f"{digest} *{archive.name}\n"
            sums.write_text(entry)
            smoke.verify_checksum(archive, sums)
            sums.write_text(entry * 2)
            with self.assertRaisesRegex(ValueError, "exactly one"):
                smoke.verify_checksum(archive, sums)
            sums.write_text(f"{'0' * 64}  {archive.name}\n")
            with self.assertRaisesRegex(ValueError, "mismatch"):
                smoke.verify_checksum(archive, sums)

    def test_command_failure_exit_status_is_not_hidden(self):
        with patch.object(smoke, "parse_args"), patch.object(smoke, "smoke") as action:
            action.side_effect = subprocess.CalledProcessError(17, ["installer"])
            self.assertEqual(smoke.main([]), 17)
            action.side_effect = subprocess.TimeoutExpired(["installer"], 900)
            self.assertEqual(smoke.main([]), 124)


if __name__ == "__main__":
    unittest.main()
