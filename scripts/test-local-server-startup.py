"""POSIX subprocess integration: built Rust hosts + real Python task/resource lifecycle.

Run with the runtime project's Python environment after building both Rust bins.
Only the chat backend is faked. All state belongs to temporary runtime homes.
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import socket
import subprocess
import sys
import tempfile
import time
import tomllib
import unittest
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent
MODEL = "7" * 64
CLI = ROOT / "target/debug/tentgent"
DAEMON = ROOT / "target/debug/tentgent-daemon"


def request(port, path, body=None):
    req = Request(
        f"http://127.0.0.1:{port}{path}",
        data=None if body is None else json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urlopen(req, timeout=5) as response:
            return response.status, json.load(response)
    except HTTPError as error:
        return error.code, json.load(error)


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for(predicate, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = predicate()
            if value:
                return value
        except (URLError, ConnectionError, FileNotFoundError):
            pass
        time.sleep(0.05)
    raise AssertionError("timed out waiting for test condition")


class LocalStartupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="tentgent-local-startup-")
        self.home = Path(self.temp.name).resolve()
        self.children = []
        self.logs = []
        self.env = {
            k: v for k, v in os.environ.items() if not k.startswith("TENTGENT_")
        }
        self.env.update(
            TENTGENT_HOME=str(self.home),
            TENTGENT_DATA_ROOT=str(self.home),
            TENTGENT_PYTHON_DIR=str(ROOT / "python/tentgent-model-runtime"),
            TENTGENT_PYTHON_ENV_DIR=str(self.home / "fake-env"),
            NO_PROXY="127.0.0.1,localhost",
        )
        entrypoint = self.home / "fake-env/bin/tentgent-model-runtime-daemon"
        entrypoint.parent.mkdir(parents=True)
        entrypoint.write_text(
            "#!/bin/sh\nexec "
            + shlex.quote(sys.executable)
            + " "
            + shlex.quote(str(ROOT / "scripts/fixtures/local_startup_runtime.py"))
            + ' "$@"\n'
        )
        entrypoint.chmod(0o700)
        store = self.home / "models/store" / MODEL
        source = store / "variants/safetensors/source"
        source.mkdir(parents=True)
        (store / "model.toml").write_text(
            f'model_ref = "{MODEL}"\nshort_ref = "{MODEL[:12]}"\n'
            'source_kind = "local"\nprimary_format = "safetensors"\n'
            'detected_formats = ["safetensors"]\nmodel_capabilities = ["chat"]\n'
            'model_capability_source = "explicit-user"\nfile_count = 3\ntotal_bytes = 6\n'
            'imported_at = "2026-09-28T00:00:00Z"\n'
        )
        (store / "manifest.json").write_text("{}")
        (source.parent / "variant.toml").write_text(
            'format = "safetensors"\nstatus = "imported"\nimport_method = "add"\n'
            'relative_source_path = "source"\n'
        )
        for name in ["config.json", "tokenizer.json", "model.safetensors"]:
            (source / name).write_text("{}")
        self.control()

    def control(self, **values):
        (self.home / "control.json").write_text(json.dumps(values))

    def command(self, *args, check=True):
        result = subprocess.run(
            [str(CLI), *args],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=35,
            check=False,
        )
        if check:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def spawn(self, binary, *args):
        log = (self.home / f"host-{len(self.children)}.log").open("w+")
        self.logs.append(log)
        child = subprocess.Popen(
            [str(binary), *args], env=self.env, stdout=log, stderr=log
        )
        self.children.append(child)
        return child

    def cli_start(self, *, lazy=False, detached=False, model_idle=0):
        port = free_port()
        args = [
            "server",
            "run",
            MODEL,
            "--port",
            str(port),
            "--allow-unverified",
            "--runtime-idle-seconds",
            "60",
            "--model-idle-seconds",
            str(model_idle),
        ]
        if lazy:
            args.append("--lazy-load")
        if detached:
            return port, self.command(*args, "--detach")
        return port, self.spawn(CLI, *args)

    def events(self, kind):
        path = self.home / "backend-events.jsonl"
        if not path.exists():
            return []
        return [
            item
            for line in path.read_text().splitlines()
            if (item := json.loads(line))["event"] == kind
        ]

    def proofs(self):
        return [
            tomllib.loads(path.read_text())
            for path in (self.home / "models/store" / MODEL).rglob("*.toml")
            if "proof" in str(path)
        ]

    def assert_proof(self, status, source="server-start"):
        proofs = self.proofs()
        self.assertTrue(proofs)
        self.assertTrue(
            all(p["status"] == status and p["source"] == source for p in proofs), proofs
        )
        self.assertTrue(
            all(p["runtime_profile"] == "local-chat-transformers-peft" for p in proofs),
            proofs,
        )

    def ready(self, port):
        return request(port, "/healthz")[1].get("ready") is True

    def python_health(self):
        paths = list((self.home / "runtime/model-runtime-daemons").glob("*/*.toml"))
        self.assertEqual(len(paths), 1)
        meta = tomllib.loads(paths[0].read_text())
        code, body = request(meta["port"], "/healthz")
        self.assertEqual(code, 200)
        self.assertEqual(body["process_token"], meta["process_token"])
        self.assertEqual(body["pid"], meta["pid"])
        return body

    def daemon_start(self):
        port = free_port()
        self.spawn(DAEMON, "--home", str(self.home), "--port", str(port))
        wait_for(lambda: request(port, "/healthz")[0] == 200)
        return port

    def rest_create(self, daemon_port):
        port = free_port()
        code, body = request(
            daemon_port,
            "/v1/servers",
            {
                "runtime_ref": MODEL,
                "port": port,
                "allow_unverified": True,
                "runtime_idle_seconds": 60,
            },
        )
        self.assertIn(code, (200, 201), body)
        return port, "/v1/servers/" + body["server"]["server_ref"]

    def test_cli_foreground_eager_preloads_then_ready_without_exit_overwriting_proof(
        self,
    ):
        self.control(blocked=True)
        port, child = self.cli_start()
        wait_for(lambda: self.events("load"))
        self.assertFalse(self.ready(port))
        self.assertEqual(request(port, "/v1/chat", {})[0], 503)
        self.assertEqual(self.proofs(), [])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.ready(port))
        self.assert_proof("verified")
        self.assertEqual(len(self.events("release")), 1)
        ref = request(port, "/healthz")[1]["server_ref"]
        self.command("server", "stop", ref)
        child.wait(timeout=5)
        self.assert_proof("verified")
        self.assertEqual(self.python_health()["status"], "ok")

    def test_cli_detached_pending_then_reuse_still_preloads_with_first_spawner_policy(
        self,
    ):
        self.control(blocked=True)
        port, result = self.cli_start(detached=True)
        self.assertIn("starting (observation expired", result.stdout)
        self.assertFalse(self.ready(port))
        self.assertEqual(self.proofs(), [])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.ready(port))
        other_port, result = self.cli_start(detached=True, model_idle=30)
        self.assertIn("readiness: ready", result.stdout)
        self.assertTrue(self.ready(other_port))
        self.assertEqual(len(self.events("spawn")), 1)
        self.assertEqual(len(self.events("preload")), 2)
        self.assertEqual(
            len(self.events("release")), 2
        )  # Reused policy remains idle 0.
        self.assert_proof("verified")

    def test_cli_lazy_health_does_not_load_and_first_request_does(self):
        port, _ = self.cli_start(lazy=True)
        wait_for(lambda: self.ready(port))
        for _ in range(3):
            self.assertTrue(self.ready(port))
        self.assertEqual(self.events("spawn"), [])
        self.assertEqual(self.proofs(), [])
        code, body = request(
            port, "/v1/chat", {"messages": [{"role": "user", "content": "hi"}]}
        )
        self.assertEqual(code, 200, body)
        self.assertEqual(body["text"], "fixture response")
        self.assertEqual(len(self.events("load")), 1)
        self.assertEqual(self.events("preload"), [])
        self.assertEqual(len(self.events("release")), 1)
        wait_for(lambda: self.proofs())
        self.assert_proof("verified", "runtime-execution")

    def test_eager_positive_model_idle_releases_despite_health_polling(self):
        port, _ = self.cli_start(model_idle=2)
        wait_for(lambda: self.ready(port))
        self.assertEqual(len(self.events("loaded")), 1)
        self.assertEqual(self.events("release"), [])
        deadline = time.monotonic() + 5
        while not self.events("release") and time.monotonic() < deadline:
            self.assertTrue(self.ready(port))
            self.assertEqual(self.python_health()["status"], "ok")
            time.sleep(0.1)
        self.assertEqual(len(self.events("release")), 1)
        self.assertEqual(len(self.events("load")), 1)
        self.assert_proof("verified")

    def test_rest_nonwaiting_and_stop_during_load_preserve_shared_python_work(self):
        self.control(blocked=True)
        daemon = self.daemon_start()
        port, route = self.rest_create(daemon)
        code, body = request(
            daemon, route + "/start", {"wait_ready": False, "allow_unverified": True}
        )
        self.assertEqual(code, 200, body)
        self.assertNotIn("readiness", body)
        wait_for(lambda: self.events("load"))
        self.assertFalse(self.ready(port))
        self.assertEqual(self.proofs(), [])
        self.assertEqual(request(daemon, route + "/stop", {})[0], 200)
        self.assertEqual(self.python_health()["status"], "ok")
        self.assertEqual(self.events("release"), [])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.events("release"))
        self.assertEqual(self.proofs(), [])

    def test_rest_wait_expiry_then_worker_completion_and_ready_reuse(self):
        self.control(blocked=True)
        daemon = self.daemon_start()
        port, route = self.rest_create(daemon)
        code, body = request(
            daemon,
            route + "/start",
            {
                "wait_ready": True,
                "timeout_seconds": 1,
                "allow_unverified": True,
            },
        )
        self.assertEqual(code, 200, body)
        self.assertFalse(body["readiness"]["ready"])
        self.assertTrue(body["server"]["running"])
        self.assertEqual(self.proofs(), [])
        code, health = request(daemon, route + "/health")
        self.assertTrue(health["reachable"])
        self.assertFalse(health["ready"])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.ready(port))
        self.assert_proof("verified")
        _, other_route = self.rest_create(daemon)
        code, body = request(
            daemon, other_route + "/start", {"wait_ready": True, "timeout_seconds": 3}
        )
        self.assertEqual(code, 200, body)
        self.assertTrue(body["readiness"]["ready"])
        self.assertEqual(len(self.events("spawn")), 1)
        self.assertEqual(len(self.events("preload")), 2)

    def test_rest_terminal_load_failure_exits_worker_and_records_failed(self):
        self.control(fail=True)
        daemon = self.daemon_start()
        _, route = self.rest_create(daemon)
        code, body = request(
            daemon,
            route + "/start",
            {
                "wait_ready": True,
                "timeout_seconds": 2,
                "allow_unverified": True,
            },
        )
        self.assertEqual(code, 200, body)
        self.assertFalse(body["readiness"]["ready"])
        self.assertFalse(body["server"]["running"])
        self.assert_proof("failed")
        self.assertEqual(len(self.events("release")), 1)
        self.assertEqual(self.python_health()["status"], "ok")

    def test_preload_timeout_exits_local_worker_without_failed_proof_or_canceling_load(
        self,
    ):
        self.control(blocked=True, preload_wait_seconds=0.1)
        _, child = self.cli_start()
        wait_for(lambda: self.events("load"))
        self.assertNotEqual(child.wait(timeout=5), 0)
        self.assertEqual(self.proofs(), [])
        self.assertEqual(self.python_health()["status"], "ok")
        self.assertEqual(self.events("release"), [])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.events("release"))
        self.assertEqual(self.proofs(), [])

    def tearDown(self):
        # Report isolated worker diagnostics before deleting a failed fixture.
        result = self._outcome.result
        if any(test is self for test, _ in result.failures + result.errors):
            for pattern in [
                "host-*.log",
                "runtime/model-runtime-daemons/**/*.log",
                "servers/**/*.log",
            ]:
                for path in self.home.glob(pattern):
                    print(
                        f"\n{path.relative_to(self.home)}:\n{path.read_text()[-6000:]}",
                        file=sys.stderr,
                    )
        # Open fixture gates before stopping hosts; only touch this test's home.
        (self.home / "allow-load").touch()
        try:
            (self.home / "allow-stream").touch()
            for path in (self.home / "servers").glob("*/server.toml"):
                spec = tomllib.loads(path.read_text())
                self.command("server", "stop", spec["server_ref"], check=False)
            for path in (self.home / "runtime/model-runtime-daemons").glob("*/*.toml"):
                meta = tomllib.loads(path.read_text())
                try:
                    _, body = request(meta["port"], "/healthz")
                    if (
                        body.get("process_token") == meta["process_token"]
                        and body.get("pid") == meta["pid"]
                    ):
                        request(meta["port"], "/v1/lifecycle/shutdown", {})
                        wait_for(
                            lambda port=meta["port"]: self.runtime_stopped(port),
                            seconds=8,
                        )
                except URLError:
                    pass
        finally:
            for child in self.children:
                if child.poll() is None:
                    child.terminate()
                child.wait(timeout=8)
            for log in self.logs:
                log.close()
            self.temp.cleanup()

    @staticmethod
    def runtime_stopped(port):
        try:
            request(port, "/healthz")
            return False
        except (URLError, ConnectionError):
            return True


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=CLI)
    parser.add_argument("--daemon", type=Path, default=DAEMON)
    args, tests = parser.parse_known_args()
    CLI, DAEMON = args.cli.resolve(), args.daemon.resolve()
    if os.name != "posix" or not CLI.is_file() or not DAEMON.is_file():
        parser.error("requires POSIX and built tentgent/tentgent-daemon binaries")
    unittest.main(argv=[sys.argv[0], *tests], verbosity=2)
