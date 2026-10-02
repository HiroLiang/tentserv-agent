"""POSIX eager reload integration: Rust hosts, real Python lifecycle, fake models.

Run sequentially with the startup suites; isolated homes still share host ports.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import sys
import time
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.request import Request, urlopen

spec = importlib.util.spec_from_file_location(
    "cluster_startup", Path(__file__).with_name("test-cluster-server-startup.py")
)
cluster = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cluster)
local = cluster.local
request, wait_for = local.request, local.wait_for
A, B, C = local.MODEL, "8" * 64, "9" * 64


class ClusterReloadTests(cluster.ClusterStartupTests):
    def setUp(self):
        super().setUp()
        for model in [B, C]:
            store = self.home / "models/store" / model
            shutil.copytree(self.home / "models/store" / A, store)
            path = store / "model.toml"
            path.write_text(
                path.read_text().replace(A, model).replace(A[:12], model[:12])
            )
        self.control(include_model=True)

    def definition(self, model, **routes):
        path = self.home / "clusters/startup/cluster.toml"
        temporary = path.with_suffix(".pending")
        temporary.write_text(
            'schema_version = 1\ncluster_ref = "startup"\nroute_update_policy = "drain"\n'
            + "".join(
                f'\n[routes.{cap}]\nkind = "local-model"\nmodel_ref = "{ref}"\n'
                for cap, ref in {"chat": model, **routes}.items()
            )
        )
        temporary.replace(path)

    def chat(self, port, model):
        code, body = request(
            port, "/v1/chat", {"messages": [{"role": "user", "content": "hi"}]}
        )
        self.assertEqual(code, 200, body)
        self.assertEqual(body["text"], f"fixture {model}")

    def loaded(self, model):
        return [event for event in self.events("load") if event["model_ref"] == model]

    def status(self, port, expected):
        return request(port, "/healthz")[1]["reload"]["status"] == expected

    def start_ready(self):
        port, child = self.cluster_start()
        wait_for(lambda: self.ready(port))
        return port, child, request(port, "/healthz")[1]

    def test_reload_keeps_old_traffic_and_stream_until_candidate_is_ready(self):
        port, _, old = self.start_ready()
        self.control(include_model=True, block_model=B, block_stream=True)
        self.definition(B)
        wait_for(lambda: self.loaded(B))
        self.chat(port, A)
        req = Request(
            f"http://127.0.0.1:{port}/v1/chat/stream",
            data=json.dumps({"messages": [{"role": "user", "content": "hi"}]}).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urlopen(req, timeout=10) as stream:
            wait_for(lambda: self.events("stream_wait"))
            health = request(port, "/healthz")[1]
            self.assertEqual(health["definition_hash"], old["definition_hash"])
            self.assertEqual(health["reload"]["status"], "preparing")
            self.assertEqual(len(self.claims()), 2)
            (self.home / "allow-load").touch()
            wait_for(lambda: self.status(port, "idle"))
            self.chat(port, B)
            self.assertEqual(len(self.claims()), 2)  # A's open response still owns A.
            (self.home / "allow-stream").touch()
            self.assertIn(A, stream.read().decode())
        wait_for(lambda: len(self.claims()) == 1)

    def test_reload_partial_failure_rolls_back_and_does_not_retry_unchanged_file(self):
        port, _, old = self.start_ready()
        self.control(include_model=True, fail_capability="embedding")
        self.definition(B, embedding=C)
        wait_for(lambda: self.status(port, "failed"))
        health = request(port, "/healthz")[1]
        self.assertEqual(health["definition_hash"], old["definition_hash"])
        self.assertIn("embedding", health["reload"]["diagnostic"])
        self.chat(port, A)
        wait_for(lambda: len(self.claims()) == 1)
        attempts = len(self.events("preload"))
        time.sleep(2.2)
        self.assertEqual(len(self.events("preload")), attempts)
        self.control(include_model=True)
        self.definition(C)
        wait_for(lambda: self.status(port, "idle"))
        self.chat(port, C)

    def test_reload_rollback_waits_for_old_stream_then_retries_without_reapply(self):
        self.definition(A)
        port, _, old = self.start_ready()
        self.control(include_model=True, block_stream=True)
        req = Request(
            f"http://127.0.0.1:{port}/v1/chat/stream",
            data=json.dumps({"messages": [{"role": "user", "content": "hi"}]}).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urlopen(req, timeout=10) as stream:
            wait_for(lambda: self.events("stream_wait"))
            self.definition(B)
            wait_for(lambda: self.loaded(B))
            wait_for(lambda: self.status(port, "idle"))
            self.definition(A)
            wait_for(lambda: self.status(port, "waiting"))
            self.chat(port, B)
            self.assertEqual(len(self.claims()), 2)
            (self.home / "allow-stream").touch()
            self.assertIn(A, stream.read().decode())
        # No second write/apply: the same candidate resumes after safe drain.
        wait_for(lambda: self.status(port, "idle"))
        self.assertEqual(
            request(port, "/healthz")[1]["definition_hash"], old["definition_hash"]
        )
        self.chat(port, A)
        self.assertEqual(len(self.claims()), 1)

    def test_reload_superseded_b_cannot_replace_committed_a(self):
        port, _, old = self.start_ready()
        self.control(include_model=True, block_model=B)
        self.definition(B)
        wait_for(lambda: self.loaded(B))
        self.control(include_model=True, block_model=C)
        self.definition(C)
        (self.home / "allow-load").touch()
        wait_for(lambda: any(e["model_ref"] == B for e in self.events("loaded")))
        # B has completed but the final disk comparison must reject it.
        wait_for(lambda: self.loaded(C))
        wait_for(lambda: self.status(port, "idle"))
        self.chat(port, C)
        self.assertNotEqual(
            request(port, "/healthz")[1]["definition_hash"], old["definition_hash"]
        )
        self.assertEqual(len(self.claims()), 1)

    def test_reload_stop_waits_for_candidate_without_promotion_or_next_route(self):
        _port, child, old = self.start_ready()
        self.control(include_model=True, block_model=B)
        self.definition(B, embedding=C)
        wait_for(lambda: self.loaded(B))
        with ThreadPoolExecutor(max_workers=1) as pool:
            stopped = pool.submit(self.command, "server", "stop", old["server_ref"])
            time.sleep(0.3)
            self.assertIsNone(child.poll())
            (self.home / "allow-load").touch()
            stopped.result(timeout=8)
        child.wait(timeout=5)
        self.assertEqual(self.loaded(C), [])
        self.assertEqual(self.claims(), [])

    def test_reload_unknown_completion_keeps_old_routes_and_candidate_claim(self):
        port, child, old = self.start_ready()
        self.control(include_model=True, block_model=B, preload_wait_seconds=0.1)
        self.definition(B)
        wait_for(lambda: self.status(port, "failed"))
        self.chat(port, A)
        self.assertEqual(
            request(port, "/healthz")[1]["definition_hash"], old["definition_hash"]
        )
        self.assertEqual(len(self.claims()), 2)
        self.assertEqual([e for e in self.events("release") if e["model_ref"] == B], [])
        diagnostic = request(port, "/healthz")[1]["reload"]["diagnostic"]
        self.assertIn(f"tentgent server stop {old['server_ref']}", diagnostic)
        self.assertIn("owning server is still running", diagnostic)
        attempts = len(self.events("preload"))
        self.definition(B)  # Reapply cannot treat unknown work as a drain retry.
        time.sleep(1.2)
        self.assertEqual(len(self.events("preload")), attempts)
        self.assertTrue(self.status(port, "failed"))
        self.command("runtime", "reconcile", "--apply")
        self.assertEqual(len(self.claims()), 2)  # Live owner claims are not stale.
        self.command("server", "stop", old["server_ref"])
        child.wait(timeout=5)
        self.assertEqual([e for e in self.events("release") if e["model_ref"] == B], [])
        # Stopping the Rust owner must not cancel accepted Python loading.
        (self.home / "allow-load").touch()
        wait_for(lambda: any(e["model_ref"] == B for e in self.events("release")))
        self.command("runtime", "reconcile", "--apply")
        self.assertEqual(self.claims(), [])
        self.control(include_model=True)
        self.command("server", "start", old["server_ref"], "--allow-unverified")
        wait_for(lambda: self.ready(port))
        self.chat(port, B)
        self.assertEqual(len(self.claims()), 1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=local.CLI)
    parser.add_argument("--daemon", type=Path, default=local.DAEMON)
    args, tests = parser.parse_known_args()
    local.CLI, local.DAEMON = args.cli.resolve(), args.daemon.resolve()
    if os.name != "posix" or not local.CLI.is_file() or not local.DAEMON.is_file():
        parser.error("requires POSIX and built tentgent/tentgent-daemon binaries")
    loader = unittest.TestLoader()
    loader.testMethodPrefix = "test_reload_"
    unittest.main(argv=[sys.argv[0], *tests], testLoader=loader, verbosity=2)
