"""Opt-in, isolated managed-runtime migration using already installed Python.

No global interpreter or repository .venv is changed. Run with an old managed
3.13 interpreter and cached base dependencies, for example:
python scripts/test-managed-python-upgrade.py --old-python /path/to/python3.13
"""

import argparse
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def run(command, *, env, cwd):
    result = subprocess.run(
        command,
        env=env,
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    if result.returncode:
        raise RuntimeError(
            f"command failed: {command!r}\n{result.stdout}\n{result.stderr}"
        )
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old-python", required=True)
    parser.add_argument("--uv", default=shutil.which("uv"))
    args = parser.parse_args()
    if not args.uv:
        parser.error("uv is required")
    if os.name != "posix":
        parser.error("this real migration test uses the POSIX bootstrap")
    with tempfile.TemporaryDirectory(prefix="tentgent-python-upgrade-") as temp:
        home = Path(temp).resolve()
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith(("TENTGENT_", "UV_"))
        }
        env.update(UV_OFFLINE="1", UV_PYTHON_DOWNLOADS="never")
        uv_cache = run([args.uv, "cache", "dir"], env=env, cwd=home).stdout.strip()
        runtime_env = home / "runtime/python-env"
        run(
            [args.uv, "venv", "--python", args.old_python, str(runtime_env)],
            env=env,
            cwd=home,
        )
        python = runtime_env / "bin/python"
        before = run(
            [str(python), "-c", "import sys; print(sys.version_info[:2])"],
            env=env,
            cwd=home,
        ).stdout.strip()
        if before != "(3, 13)":
            raise RuntimeError(f"expected old Python 3.13, got {before}")

        retained = {}
        for directory in (
            "models",
            "adapters",
            "datasets",
            "servers",
            "train",
            "runtime/ownership",
        ):
            marker = home / directory / "preserve.marker"
            marker.parent.mkdir(parents=True, exist_ok=True)
            marker.write_text(f"retain {directory}\n")
            retained[marker] = marker.read_bytes()

        share = home / "prefix/share/tentgent"
        project = share / "python/tentgent-model-runtime"
        project.parent.mkdir(parents=True)
        shutil.copytree(
            ROOT / "python/tentgent-model-runtime",
            project,
            ignore=shutil.ignore_patterns(
                ".venv", "__pycache__", "*.pyc", ".pytest_cache"
            ),
        )
        for name in ("pyproject.toml", "uv.lock"):
            shutil.copy2(ROOT / name, share / name)
        scripts = share / "scripts"
        scripts.mkdir()
        for name in ("bootstrap-python-env.sh", "bootstrap-uv.sh"):
            shutil.copy2(ROOT / "scripts" / name, scripts / name)
        env.update(
            TENTGENT_HOME=str(home),
            TENTGENT_BOOTSTRAP_UV_CACHE_DIR=uv_cache,
        )
        bootstrap = [
            "bash",
            str(scripts / "bootstrap-python-env.sh"),
            "--project",
            str(project),
            "--env",
            str(runtime_env),
            "--uv",
            args.uv,
            "--profile",
            "base",
        ]
        incompatible = subprocess.run(
            bootstrap,
            env={**env, "TENTGENT_BOOTSTRAP_PYTHON_VERSION": "3.13"},
            cwd=home,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        if incompatible.returncode == 0:
            raise RuntimeError("an incompatible Python 3.13 override was accepted")
        if (
            "remove an incompatible TENTGENT_BOOTSTRAP_PYTHON_VERSION override"
            not in incompatible.stderr
        ):
            raise RuntimeError(
                f"bootstrap omitted its recovery guidance: {incompatible.stderr}"
            )
        preserved = run(
            [str(python), "-c", "import sys; print(sys.version_info[:2])"],
            env=env,
            cwd=home,
        ).stdout.strip()
        if preserved != before:
            raise RuntimeError("rejected Python override changed the old interpreter")
        print(
            "PASS: incompatible 3.13 override rejected without replacing the existing environment"
        )
        result = run(
            bootstrap,
            env=env,
            cwd=home,
        )
        print(result.stdout)
        print(result.stderr)
        probe = run(
            [
                str(python),
                "-c",
                (
                    "import json, sys; from tentgent.runtime.server import app; "
                    "print(json.dumps({'version': list(sys.version_info[:2]), 'module': app.__file__}))"
                ),
            ],
            env=env,
            cwd=home,
        )
        installed = json.loads(probe.stdout)
        if installed["version"] != [3, 12]:
            raise RuntimeError(f"expected managed Python 3.12, got {installed}")
        if not Path(installed["module"]).resolve().is_relative_to(runtime_env):
            raise RuntimeError(
                f"runtime import escaped the managed environment: {installed}"
            )
        for marker, content in retained.items():
            if marker.read_bytes() != content:
                raise RuntimeError(f"bootstrap changed user state: {marker}")
        print(
            "PASS: managed Python 3.13 -> 3.12; installed runtime import and six user-state markers preserved"
        )


if __name__ == "__main__":
    main()
