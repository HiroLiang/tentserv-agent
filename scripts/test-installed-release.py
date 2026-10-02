"""Smoke-test a native release archive through the real, isolated installer.

Downloads only installer-owned uv/Python/base dependencies, never models. Run
after packaging on the native release host; no development environment is used.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    ("Darwin", "arm64"): "aarch64-apple-darwin",
    ("Darwin", "x86_64"): "x86_64-apple-darwin",
    ("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
    ("Windows", "amd64"): "x86_64-pc-windows-msvc",
    ("Windows", "x86_64"): "x86_64-pc-windows-msvc",
}

PYTHON_PROBE = """
import importlib
import importlib.metadata
import json
import sys
import sysconfig
from pathlib import Path

expected_env, repository = (Path(value).resolve() for value in sys.argv[1:])
assert sys.version_info[:2] == (3, 12), sys.version
assert Path(sys.prefix).resolve() == expected_env, sys.prefix
site_roots = {Path(sysconfig.get_path(key)).resolve() for key in ('purelib', 'platlib')}
assert all(path.is_relative_to(expected_env) for path in site_roots), site_roots
modules = {}
for name in ('tentgent.runtime.server.app', 'tentgent.runtime.server.preload',
             'tentgent.runtime.backends.model_safety'):
    module = importlib.import_module(name)
    location = Path(module.__file__).resolve()
    assert any(location.is_relative_to(path) for path in site_roots), location
    assert not location.is_relative_to(repository), location
    modules[name] = str(location)
assert callable(importlib.import_module('tentgent.runtime.server.app').create_app)
dist = importlib.metadata.distribution('tentgent-model-runtime')
direct_url = json.loads(dist.read_text('direct_url.json') or '{}')
assert not direct_url.get('dir_info', {}).get('editable', False), direct_url
print(json.dumps({'python': sys.version, 'environment': str(expected_env),
                  'runtime_version': dist.version, 'modules': modules}, indent=2))
"""


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument(
        "--target", required=True, choices=sorted(set(TARGETS.values()))
    )
    parser.add_argument("--version", required=True)
    args = parser.parse_args(argv)
    args.version = args.version.removeprefix("v")
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z][0-9A-Za-z.-]*)?", args.version):
        parser.error("--version must be a stable or prerelease version")
    return args


def verify_checksum(archive: Path, checksums: Path) -> None:
    entries = []
    for line in checksums.read_text(encoding="utf-8").splitlines():
        fields = line.split()
        if len(fields) == 2 and fields[1].removeprefix("*") == archive.name:
            entries.append(fields[0].lower())
    if len(entries) != 1 or not re.fullmatch(r"[0-9a-f]{64}", entries[0]):
        raise ValueError(f"expected exactly one SHA-256 entry for {archive.name}")
    digest = hashlib.sha256()
    with archive.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != entries[0]:
        raise ValueError(f"SHA-256 mismatch for {archive.name}")
    print(f"SHA-256 verified: {archive.name}", flush=True)


def isolated_environment(root: Path, inherited: dict[str, str]) -> dict[str, str]:
    env = {
        key: value
        for key, value in inherited.items()
        if not key.upper().startswith(("TENTGENT_", "PYTHON", "UV_", "CONDA_"))
        and key.upper() != "VIRTUAL_ENV"
    }
    env.update(
        TENTGENT_HOME=str(root / "home"),
        UV_PYTHON_INSTALL_DIR=str(root / "managed-python"),
        UV_CACHE_DIR=str(root / "uv-cache"),
        HF_HOME=str(root / "huggingface"),
        HF_HUB_OFFLINE="1",
        PYTHONNOUSERSITE="1",
        PYTHONDONTWRITEBYTECODE="1",
    )
    return env


def installer_command(args: argparse.Namespace, prefix: Path) -> list[str]:
    if args.target.endswith("windows-msvc"):
        return [
            "pwsh",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            str(ROOT / "scripts/install.ps1"),
            "-Archive",
            str(args.archive),
            "-Checksums",
            str(args.checksums),
            "-Version",
            args.version,
            "-Prefix",
            str(prefix),
            "-Target",
            args.target,
            "-SkipDoctor",
        ]
    return [
        "bash",
        str(ROOT / "scripts/install.sh"),
        "--archive",
        str(args.archive),
        "--checksums",
        str(args.checksums),
        "--version",
        args.version,
        "--prefix",
        str(prefix),
        "--target",
        args.target,
        "--skip-doctor",
    ]


def run(
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    capture: bool = False,
) -> str:
    print(f"==> {shlex.join(command)}", flush=True)
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        timeout=900,
    )
    if result.stdout:
        print(result.stdout, end="", flush=True)
    return result.stdout or ""


def verify_layout(prefix: Path, runtime_home: Path, windows: bool) -> tuple[Path, Path]:
    share = prefix / "share/tentgent"
    env_dir = runtime_home / "runtime/python-env"
    entrypoints = env_dir / ("Scripts" if windows else "bin")
    suffix = ".exe" if windows else ""
    required = [
        prefix / f"bin/tentgent{suffix}",
        share / "pyproject.toml",
        share / "uv.lock",
        share / "python/tentgent-model-runtime/pyproject.toml",
        share / "python/tentgent-model-runtime/src/tentgent/runtime/server/preload.py",
        share
        / "python/tentgent-model-runtime/src/tentgent/runtime/backends/model_safety.py",
        share / "scripts/bootstrap-python-env.sh",
        share / "scripts/bootstrap-uv.sh",
        *(
            entrypoints / f"{name}{suffix}"
            for name in (
                "python",
                "tentgent-model-runtime-daemon",
                "tentgent-hf-snapshot",
            )
        ),
    ]
    for path in required:
        if not path.is_file():
            raise ValueError(f"installed package is missing {path}")
    return env_dir, entrypoints / f"python{suffix}"


def smoke(args: argparse.Namespace) -> None:
    args.archive = args.archive.resolve(strict=True)
    args.checksums = args.checksums.resolve(strict=True)
    verify_checksum(args.archive, args.checksums)
    native = TARGETS.get((platform.system(), platform.machine().lower()))
    if native != args.target:
        raise ValueError(f"native host {native!r} cannot test target {args.target}")
    windows = args.target.endswith("windows-msvc")
    extension = "zip" if windows else "tar.gz"
    expected_name = f"tentgent-{args.version}-{args.target}.{extension}"
    if args.archive.name != expected_name:
        raise ValueError(f"archive filename must be {expected_name}")
    with tempfile.TemporaryDirectory(prefix="tentgent-installed-release-") as temporary:
        root = Path(temporary).resolve()
        if root.is_relative_to(ROOT):
            raise ValueError(
                "artifact smoke temporary directory must be outside repository"
            )
        prefix, runtime_home, cwd = root / "prefix", root / "home", root / "cwd"
        cwd.mkdir()
        runtime_home.mkdir()
        env = isolated_environment(root, dict(os.environ))
        run(installer_command(args, prefix), cwd=cwd, env=env)
        env_dir, python = verify_layout(prefix, runtime_home, windows)
        binary = prefix / ("bin/tentgent.exe" if windows else "bin/tentgent")
        version = run(
            [str(binary), "--version"], cwd=cwd, env=env, capture=True
        ).strip()
        expected_version = f"tentgent {args.version.split('-', 1)[0]}"
        if version != expected_version:
            raise ValueError(
                f"installed binary version {version!r} != {expected_version!r}"
            )
        if not windows:
            command = [str(binary), "runtime", "bootstrap", "--profile", "base"]
            plan = run([*command, "--print-plan"], cwd=cwd, env=env, capture=True)
            fields = dict(
                line.split(": ", 1) for line in plan.splitlines() if ": " in line
            )
            expected = {
                "project": prefix / "share/tentgent/python/tentgent-model-runtime",
                "env": env_dir,
                "script": prefix / "share/tentgent/scripts/bootstrap-python-env.sh",
            }
            for key, path in expected.items():
                if Path(fields.get(key, "")).resolve() != path.resolve():
                    raise ValueError(
                        f"installed CLI bootstrap resolved wrong {key}: {fields}"
                    )
            run(command, cwd=cwd, env=env)
        else:
            print(
                "Windows: validated installer bootstrap; CLI bootstrap is unsupported."
            )
        run(
            [str(python), "-I", "-c", PYTHON_PROBE, str(env_dir), str(ROOT)],
            cwd=cwd,
            env=env,
        )
        uv_name = "uv.exe" if windows else "uv"
        tools = list(
            (runtime_home / "runtime/bootstrap/uv").glob(
                f"*/{args.target}/bin/{uv_name}"
            )
        )
        if len(tools) != 1:
            raise ValueError(f"expected exactly one installer-owned uv, found {tools}")
        run(
            [str(tools[0]), "--no-config", "pip", "check", "--python", str(python)],
            cwd=cwd,
            env=env,
        )
    print(f"Installed release smoke passed: {args.version} / {args.target}", flush=True)


def main(argv: list[str] | None = None) -> int:
    try:
        smoke(parse_args(argv))
    except subprocess.CalledProcessError as error:
        print(f"installed release command failed: {error}", file=sys.stderr)
        return error.returncode if error.returncode > 0 else 1
    except subprocess.TimeoutExpired as error:
        print(f"installed release command timed out: {error}", file=sys.stderr)
        return 124
    except (OSError, ValueError) as error:
        print(f"installed release smoke failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
