"""Offline release packaging/notarization regression checks with fake commands."""

import json
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class ReleasePackageGuards(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="tentgent-release-guards-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.commands = self.root / "commands"
        self.commands.mkdir()
        self.env = dict(os.environ)
        self.env.update(PATH=f"{self.commands}{os.pathsep}{os.environ['PATH']}")
        for name in ("TENTGENT_VERSION", "TENTGENT_TARGET"):
            self.env.pop(name, None)

    def command(self, name, body):
        path = self.commands / name
        path.write_text(f"#!/usr/bin/env bash\nset -euo pipefail\n{body}\n")
        path.chmod(0o755)

    def run_script(self, script, *args):
        return subprocess.run(
            ["bash", str(script), *args],
            cwd=self.root,
            env=self.env,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )

    def package_fixture(self):
        repo = self.root / "repo"
        scripts = repo / "scripts"
        scripts.mkdir(parents=True)
        shutil.copy2(ROOT / "scripts/package-local.sh", scripts)
        (repo / "Cargo.toml").write_text('[workspace.package]\nversion = "1.2.3"\n')
        for name in ("README.md", "LICENSE", "pyproject.toml", "uv.lock"):
            (repo / name).write_text("fixture\n")
        for name in (
            "bootstrap-uv.sh",
            "bootstrap-python-env.sh",
            "install.sh",
            "install.ps1",
        ):
            (scripts / name).write_text("fixture\n")
        runtime = repo / "python/tentgent-model-runtime/src/tentgent"
        runtime.mkdir(parents=True)
        (runtime / "__init__.py").write_text("# bundled runtime\n")
        binary = repo / "target/release/tentgent"
        binary.parent.mkdir(parents=True)
        binary.write_text(
            '#!/usr/bin/env bash\nprintf "tentgent %s\\n" "$FAKE_BINARY_VERSION"\n'
        )
        binary.chmod(0o755)
        self.command(
            "uname", 'if [[ "$1" == "-s" ]]; then echo Linux; else echo x86_64; fi'
        )
        self.command(
            "cargo",
            'printf "%s\\n" "$@" >"$FAKE_CARGO_LOG"\npwd >"$FAKE_CARGO_CWD"',
        )
        self.env.update(
            FAKE_CARGO_LOG=str(self.root / "cargo.log"),
            FAKE_CARGO_CWD=str(self.root / "cargo-cwd.log"),
            FAKE_BINARY_VERSION="1.2.3",
            TENTGENT_TARGET="x86_64-unknown-linux-gnu",
        )
        return repo, scripts / "package-local.sh"

    def test_stable_and_rc_package_locked_build_and_bundled_runtime(self):
        repo, script = self.package_fixture()
        for version in ("1.2.3", "1.2.3-rc.132.1"):
            with self.subTest(version=version):
                self.env["TENTGENT_VERSION"] = version
                result = self.run_script(script)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(
                    (self.root / "cargo.log").read_text().splitlines(),
                    [
                        "build",
                        "--manifest-path",
                        str(repo / "Cargo.toml"),
                        "--release",
                        "--locked",
                        "--bin",
                        "tentgent",
                    ],
                )
                self.assertEqual(
                    Path((self.root / "cargo-cwd.log").read_text().strip()).resolve(),
                    repo.resolve(),
                )
                archive = (
                    repo
                    / "dist"
                    / f"tentgent-{version}-x86_64-unknown-linux-gnu.tar.gz"
                )
                with tarfile.open(archive) as payload:
                    self.assertIn("bin/tentgent", payload.getnames())
                    self.assertIn("share/tentgent/uv.lock", payload.getnames())
                    self.assertIn(
                        "share/tentgent/python/tentgent-model-runtime/src/tentgent/__init__.py",
                        payload.getnames(),
                    )

    def test_mismatched_package_base_rejected_before_build(self):
        repo, script = self.package_fixture()
        self.env["TENTGENT_VERSION"] = "1.2.4-rc.1"
        result = self.run_script(script)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match workspace version", result.stderr)
        self.assertFalse((self.root / "cargo.log").exists())
        self.assertFalse((repo / "dist").exists())

    def test_invalid_package_version_rejected_before_dist_mutation(self):
        repo, script = self.package_fixture()
        for version in ("v1.2.3", "1.2.3/../../unexpected", "1.2.3-"):
            with self.subTest(version=version):
                self.env["TENTGENT_VERSION"] = version
                result = self.run_script(script, "--print-plan")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("invalid package version", result.stderr)
                self.assertFalse((repo / "dist").exists())

    def test_stale_binary_version_rejected_before_staging(self):
        repo, script = self.package_fixture()
        self.env["FAKE_BINARY_VERSION"] = "1.2.2"
        result = self.run_script(script)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("binary version mismatch", result.stderr)
        self.assertFalse((repo / "dist").exists())

    def notary_fixture(self):
        payload = self.root / "payload/bin"
        payload.mkdir(parents=True)
        binary = payload / "tentgent"
        binary.write_text("fixture\n")
        binary.chmod(0o755)
        archive = self.root / "tentgent-fixture.tar.gz"
        with tarfile.open(archive, "w:gz") as packaged:
            packaged.add(payload, arcname="bin")
        self.command("uname", "echo Darwin")
        self.command(
            "codesign",
            'printf "%s\\n" "$*" >>"$FAKE_CODESIGN_LOG"\n'
            'if [[ "$1" == "-dv" ]]; then echo "TeamIdentifier=$APPLE_TEAM_ID"; fi',
        )
        self.command(
            "ditto",
            'if [[ "$1" == "-c" ]]; then touch "${!#}"; else cp -R "$1" "$2"; fi',
        )
        self.command(
            "xcrun",
            'printf "%s\\n" "$@" >"$FAKE_NOTARY_LOG"\n'
            'printf "%s" "$FAKE_NOTARY_RESULT"\nexit "$FAKE_NOTARY_EXIT"',
        )
        self.env.update(
            APPLE_NOTARY_KEY_BASE64="dGVzdC1rZXk=",
            APPLE_NOTARY_KEY_ID="test-key",
            APPLE_NOTARY_ISSUER_ID="test-issuer",
            APPLE_TEAM_ID="TESTTEAM",
            FAKE_CODESIGN_LOG=str(self.root / "codesign.log"),
            FAKE_NOTARY_LOG=str(self.root / "notary.log"),
            FAKE_NOTARY_RESULT=json.dumps(
                {"status": "Accepted", "id": "test-submission"}
            ),
            FAKE_NOTARY_EXIT="0",
        )
        return archive

    def notarize(self, archive):
        return self.run_script(
            ROOT / "scripts/macos-notarize-package.sh",
            "--archive",
            str(archive),
            "--target",
            "aarch64-apple-darwin",
        )

    def test_notary_accepted_with_id(self):
        result = self.notarize(self.notary_fixture())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("submission id: test-submission", result.stdout)
        self.assertIn("macOS package notarization accepted", result.stdout)
        self.assertEqual(
            (self.root / "notary.log").read_text().splitlines()[-3:],
            ["--wait", "--output-format", "json"],
        )
        self.assertEqual(len((self.root / "codesign.log").read_text().splitlines()), 3)

    def test_notary_rejects_nonaccepted_or_malformed_responses_even_with_exit_zero(
        self,
    ):
        archive = self.notary_fixture()
        responses = [
            {"status": "Invalid", "id": "test"},
            {"status": "Rejected", "id": "test"},
            {"status": "In Progress", "id": "test"},
            {"id": "test"},
            {"status": "Accepted"},
            {"status": "Accepted", "id": " "},
            {"status": "Accepted", "id": 123},
            [],
            "invalid JSON",
        ]
        for response in responses:
            with self.subTest(response=response):
                self.env["FAKE_NOTARY_RESULT"] = (
                    response if isinstance(response, str) else json.dumps(response)
                )
                result = self.notarize(archive)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("macOS package notarization accepted", result.stdout)

    def test_notary_nonzero_exit_cannot_be_accepted(self):
        archive = self.notary_fixture()
        self.env["FAKE_NOTARY_EXIT"] = "1"
        result = self.notarize(archive)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("submission failed", result.stderr)
        self.assertNotIn("macOS package notarization accepted", result.stdout)

    def test_powershell_and_bash_bootstrap_both_force_runtime_reinstall(self):
        # Native invocation coverage lives in test-windows-runtime-upgrade.ps1.
        # Keep this cross-platform check useful when PowerShell is unavailable.
        for script in ("install.ps1", "bootstrap-python-env.sh"):
            with self.subTest(script=script):
                self.assertIn(
                    "--reinstall-package tentgent-model-runtime",
                    (ROOT / "scripts" / script).read_text(),
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
