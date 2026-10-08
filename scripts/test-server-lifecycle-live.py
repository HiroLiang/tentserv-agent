"""Opt-in real-model #132 smoke with isolated server/runtime homes.

Requires an already imported small chat model and a working Python backend.
Does not download models, alter production servers, or remove the supplied store.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import time
import tomllib
import unittest
from pathlib import Path
from urllib.request import Request, urlopen

spec = importlib.util.spec_from_file_location(
    "local_startup", Path(__file__).with_name("test-local-server-startup.py")
)
local = importlib.util.module_from_spec(spec)
spec.loader.exec_module(local)
wait_for, free_port = local.wait_for, local.free_port


class LiveLifecycleTests(local.LocalStartupTests):
    def setUp(self):
        super().setUp()
        self.env["TENTGENT_DATA_ROOT"] = str(args.data_root.resolve())
        self.env["TENTGENT_PYTHON_ENV_DIR"] = str(Path(sys.prefix).resolve())
        self.env["TENTGENT_PYTHON_DIR"] = str(args.python_project.resolve())
        # An installed-artifact smoke must not import the developer's source
        # through inherited Python overrides.
        for name in ("PYTHONPATH", "PYTHONHOME"):
            self.env.pop(name, None)

    def tearDown(self):
        pids = [
            tomllib.loads(path.read_text())["pid"]
            for path in (self.home / "runtime/model-runtime-daemons").glob("*/*.toml")
        ]
        super().tearDown()
        for pid in pids:
            wait_for(
                lambda pid=pid: (
                    subprocess.run(
                        ["ps", "-p", str(pid)], capture_output=True, check=False
                    ).returncode
                    != 0
                ),
                seconds=5,
            )
        print(
            json.dumps(
                {
                    "case": self._testMethodName,
                    "phase": "cleanup",
                    "remaining_test_processes": 0,
                }
            ),
            flush=True,
        )

    def start(self, *, lazy=False, model_idle=0, runtime_idle=12):
        port = free_port()
        command = [
            "server",
            "run",
            args.model_ref,
            "--port",
            str(port),
            "--capability",
            "chat",
            "--allow-unverified",
            "--runtime-idle-seconds",
            str(runtime_idle),
            "--model-idle-seconds",
            str(model_idle),
        ]
        if lazy:
            command.append("--lazy-load")
        child = self.spawn(local.CLI, *command)
        wait_for(lambda: self.ready(port), seconds=60)
        self.assertIsNone(child.poll())
        return port

    def chat(self, port):
        req = Request(
            f"http://127.0.0.1:{port}/v1/chat",
            data=json.dumps(
                {
                    "messages": [{"role": "user", "content": "Say hello."}],
                    "max_tokens": 12,
                    "temperature": 0.0,
                }
            ).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urlopen(req, timeout=60) as response:
            body = json.load(response)
            self.assertEqual(response.status, 200)
            self.assertTrue(body["text"].strip(), body)
        return body["text"]

    def meta(self):
        paths = list((self.home / "runtime/model-runtime-daemons").glob("*/*.toml"))
        self.assertEqual(len(paths), 1)
        return tomllib.loads(paths[0].read_text())

    def sample(self, label):
        health = self.python_health()
        resources = health["runtime"]["resources"]
        rss = subprocess.run(
            ["ps", "-p", str(health["pid"]), "-o", "rss="],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        evidence = {
            "case": self._testMethodName,
            "phase": label,
            "pid": health["pid"],
            "rss_kib": int(rss),
            "resources": resources,
            "policy": health["runtime"]["lifecycle"],
        }
        print(json.dumps(evidence), flush=True)
        self.assertEqual(
            sum(item["active_leases"] for item in resources["model_resources"]), 0
        )
        return health

    def wait_exit_with_health_polling(self, proxy, meta):
        deadline = time.monotonic() + 22
        while time.monotonic() < deadline:
            self.assertTrue(self.ready(proxy))
            # This already polls Python health and handles a reset/refusal as
            # shutdown; a second unguarded request races the expected exit.
            if self.runtime_stopped(meta["port"]):
                break
            time.sleep(0.2)
        self.assertTrue(self.runtime_stopped(meta["port"]))
        wait_for(
            lambda: (
                subprocess.run(
                    ["ps", "-p", str(meta["pid"])], capture_output=True, check=False
                ).returncode
                != 0
            ),
            seconds=5,
        )
        print(
            json.dumps(
                {
                    "case": self._testMethodName,
                    "phase": "runtime-exited",
                    "pid": meta["pid"],
                    "health_polling": True,
                }
            ),
            flush=True,
        )

    def test_live_eager_zero_releases_preload_and_inference_then_restarts_after_idle(
        self,
    ):
        port = self.start()
        health = self.sample("eager-ready")
        self.assertEqual(health["runtime"]["resources"]["model_resource_count"], 0)
        print("chat:", self.chat(port), flush=True)
        self.assertEqual(
            self.sample("after-inference")["runtime"]["resources"][
                "model_resource_count"
            ],
            0,
        )
        previous = self.meta()
        self.wait_exit_with_health_polling(port, previous)
        print("restarted chat:", self.chat(port), flush=True)
        current = self.meta()
        self.assertNotEqual(previous["process_token"], current["process_token"])
        self.assertNotEqual(previous["pid"], current["pid"])
        self.assertEqual(
            self.sample("restarted")["runtime"]["resources"]["model_resource_count"], 0
        )

    def test_live_positive_retention_reused_generation_keeps_first_policy(self):
        port = self.start(model_idle=4)
        first = self.sample("retained-after-preload")
        self.assertEqual(first["runtime"]["resources"]["model_resource_count"], 1)
        self.assertTrue(first["runtime"]["resources"]["model_resources"][0]["loaded"])
        second_port = self.start(model_idle=8, runtime_idle=20)
        reused = self.sample("reused-after-preload")
        self.assertEqual(first["process_token"], reused["process_token"])
        self.assertEqual(
            reused["runtime"]["lifecycle"],
            {"runtime_idle_seconds": 12, "model_idle_seconds": 4},
        )
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            self.assertTrue(self.ready(port) and self.ready(second_port))
            if (
                self.python_health()["runtime"]["resources"]["model_resource_count"]
                == 0
            ):
                break
            time.sleep(0.2)
        self.assertEqual(
            self.sample("retention-expired")["runtime"]["resources"][
                "model_resource_count"
            ],
            0,
        )
        self.wait_exit_with_health_polling(port, self.meta())

    def test_live_lazy_health_creates_no_runtime_until_first_request(self):
        port = self.start(lazy=True)
        for _ in range(5):
            self.assertTrue(self.ready(port))
            time.sleep(0.1)
        self.assertEqual(
            list((self.home / "runtime/model-runtime-daemons").glob("*/*.toml")), []
        )
        print("lazy first chat:", self.chat(port), flush=True)
        self.assertEqual(
            self.sample("lazy-after-first-request")["runtime"]["resources"][
                "model_resource_count"
            ],
            0,
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-ref", required=True)
    parser.add_argument("--data-root", type=Path, required=True)
    parser.add_argument("--cli", type=Path, default=local.CLI)
    parser.add_argument(
        "--python-project",
        type=Path,
        default=local.ROOT / "python/tentgent-model-runtime",
        help="packaged Python project when testing an installed release",
    )
    args, tests = parser.parse_known_args()
    local.CLI = args.cli.resolve()
    if os.name != "posix" or not local.CLI.is_file():
        parser.error("requires POSIX and a built tentgent binary")
    if not (args.data_root / "models/store" / args.model_ref / "model.toml").is_file():
        parser.error(
            "model-ref must be a full reference present in the supplied test data root"
        )
    if not (args.python_project / "pyproject.toml").is_file():
        parser.error(
            "python-project must contain the installed or source pyproject.toml"
        )
    loader = unittest.TestLoader()
    loader.testMethodPrefix = "test_live_"
    unittest.main(argv=[sys.argv[0], *tests], testLoader=loader, verbosity=2)
