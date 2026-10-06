"""POSIX Cluster startup integration with Rust hosts and real Python lifecycle.

Reuses the isolated Local startup harness; only backend load/chat is faked.
No real model weights are downloaded or loaded.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import socket
import sys
import time
import tomllib
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "local_startup", Path(__file__).with_name("test-local-server-startup.py")
)
local = importlib.util.module_from_spec(spec)
spec.loader.exec_module(local)
request, wait_for, free_port = local.request, local.wait_for, local.free_port
CAPABILITIES = ["chat", "embedding", "rerank", "audio-transcription", "vision-chat"]


class ClusterStartupTests(local.LocalStartupTests):
    def setUp(self):
        super().setUp()
        store = self.home / "models/store" / local.MODEL
        metadata = store / "model.toml"
        metadata.write_text(
            metadata.read_text().replace(
                'model_capabilities = ["chat"]',
                "model_capabilities = ["
                + ", ".join(f'"{cap}"' for cap in CAPABILITIES)
                + "]",
            )
        )
        (store / "variants/safetensors/source/preprocessor_config.json").write_text(
            "{}"
        )
        self.cluster_definition(["chat"])

    def cluster_definition(self, capabilities):
        path = self.home / "clusters/startup/cluster.toml"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            'schema_version = 1\ncluster_ref = "startup"\n'
            + "".join(
                f'\n[routes.{cap}]\nkind = "local-model"\nmodel_ref = "{local.MODEL}"\n'
                for cap in capabilities
            )
        )

    def cluster_start(self, *, lazy=False, detached=False, model_idle=0):
        port = free_port()
        args = [
            "cluster",
            "run",
            "startup",
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
        return port, self.spawn(local.CLI, *args)

    def claims(self):
        return list((self.home / "runtime").glob("*/claims/*.toml"))

    def hidden_cluster(self, binary, port):
        return self.spawn(
            binary,
            "__cluster-server-runtime",
            "--server-ref",
            "fixture-worker",
            "--cluster-ref",
            "startup",
            "--host",
            "127.0.0.1",
            "--port",
            str(port),
            "--home",
            str(self.home),
            "--allow-unverified",
        )

    def runtime_healths(self):
        results = []
        for path in (self.home / "runtime/model-runtime-daemons").glob("*/*.toml"):
            meta = tomllib.loads(path.read_text())
            code, body = request(meta["port"], "/healthz")
            self.assertEqual(code, 200)
            self.assertEqual(body["process_token"], meta["process_token"])
            results.append(body)
        return results

    def test_cluster_all_routes_preload_release_and_ready_as_one_unit(self):
        self.cluster_definition(CAPABILITIES)
        self.control(block_capability="embedding")
        port, child = self.cluster_start()
        wait_for(lambda: len(self.events("load")) == 2)
        health = request(port, "/healthz")[1]
        self.assertFalse(health["ok"] or health["ready"])
        self.assertEqual(health["status"], "starting")
        self.assertEqual(health["load_mode"], "eager")
        self.assertEqual(len(self.claims()), 2)
        for path in [
            "/v1/chat",
            "/v1/embeddings",
            "/v1/rerank",
            "/v1/audio/transcriptions",
            "/v1/vision/chat",
        ]:
            self.assertEqual(request(port, path, {})[0], 503, path)
        (self.home / "allow-load").touch()
        wait_for(lambda: self.ready(port), seconds=25)
        self.assertEqual(
            [event["capability"] for event in self.events("preload")], CAPABILITIES
        )
        self.assertEqual(len(self.events("release")), 5)
        self.assertEqual(len(self.claims()), 5)
        # The store contains both current proofs and immutable attempt history.
        self.assertEqual(
            {proof["capability"] for proof in self.proofs()}, set(CAPABILITIES)
        )
        self.assertTrue(all(proof["status"] == "verified" for proof in self.proofs()))
        health = request(port, "/healthz")[1]
        self.assertEqual(health["ownership"]["active_request_count"], 0)
        self.command("server", "stop", health["server_ref"])
        child.wait(timeout=5)
        self.assertEqual(self.claims(), [])
        self.assertEqual(len(self.runtime_healths()), 5)  # Shared generations survive.

    def test_cluster_detached_observation_expiry_and_reuse_keep_first_policy(self):
        self.control(blocked=True)
        port, result = self.cluster_start(detached=True)
        self.assertIn("starting (observation expired", result.stdout)
        self.assertFalse(self.ready(port))
        self.assertEqual(self.proofs(), [])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.ready(port))
        other, result = self.cluster_start(detached=True, model_idle=30)
        self.assertIn("readiness: ready", result.stdout)
        self.assertTrue(self.ready(other))
        self.assertEqual(len(self.events("spawn")), 1)
        self.assertEqual(len(self.events("preload")), 2)
        self.assertEqual(len(self.events("release")), 2)
        self.assertEqual(len(self.claims()), 2)

    def test_cluster_lazy_health_does_not_load_first_request_does(self):
        port, _ = self.cluster_start(lazy=True)
        wait_for(lambda: self.ready(port))
        for _ in range(3):
            self.assertTrue(self.ready(port))
        self.assertEqual(self.events("spawn"), [])
        self.assertEqual(self.claims(), [])
        code, body = request(
            port, "/v1/chat", {"messages": [{"role": "user", "content": "hi"}]}
        )
        self.assertEqual(code, 200, body)
        self.assertEqual(body["text"], "fixture response")
        self.assertEqual(self.events("preload"), [])
        self.assertEqual(len(self.events("release")), 1)

    def test_cluster_positive_model_idle_releases_without_dropping_route_claim(self):
        port, _ = self.cluster_start(model_idle=2)
        wait_for(lambda: self.ready(port))
        self.assertEqual(self.events("release"), [])
        deadline = time.monotonic() + 5
        while not self.events("release") and time.monotonic() < deadline:
            self.assertTrue(self.ready(port))
            self.runtime_healths()
            time.sleep(0.1)
        self.assertEqual(len(self.events("release")), 1)
        self.assertEqual(len(self.events("load")), 1)
        self.assertEqual(len(self.claims()), 1)

    def test_cluster_rest_wait_timeout_is_observational_then_ready(self):
        self.control(blocked=True)
        daemon = self.daemon_start()
        port = free_port()
        code, body = request(
            daemon,
            "/v1/servers",
            {
                "runtime_kind": "cluster",
                "cluster_ref": "startup",
                "port": port,
                "allow_unverified": True,
                "runtime_idle_seconds": 60,
            },
        )
        self.assertIn(code, (200, 201), body)
        route = "/v1/servers/" + body["server"]["server_ref"]
        code, body = request(
            daemon,
            route + "/start",
            {"wait_ready": True, "timeout_seconds": 1, "allow_unverified": True},
        )
        self.assertEqual(code, 200, body)
        self.assertFalse(body["readiness"]["ready"])
        self.assertTrue(body["server"]["running"])
        wait_for(lambda: self.events("load"))
        self.assertEqual(self.proofs(), [])
        self.assertFalse(request(daemon, route + "/health")[1]["ready"])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.ready(port))
        self.assertTrue(request(daemon, route + "/health")[1]["ready"])

    def test_cluster_partial_terminal_failure_releases_claims_not_shared_runtimes(self):
        self.cluster_definition(["chat", "embedding", "rerank"])
        self.control(fail_capability="embedding")
        _, child = self.cluster_start()
        self.assertNotEqual(child.wait(timeout=20), 0)
        self.assertEqual(
            [e["capability"] for e in self.events("preload")], ["chat", "embedding"]
        )
        self.assertEqual(len(self.events("release")), 2)
        self.assertEqual(self.claims(), [])
        self.assertEqual(
            {p["capability"]: p["status"] for p in self.proofs()},
            {"chat": "verified", "embedding": "failed"},
        )
        self.assertEqual(len(self.runtime_healths()), 2)
        logs = "".join(path.read_text() for path in self.home.glob("host-*.log"))
        self.assertIn("eager startup route(s) [embedding]", logs)

    def test_cluster_preload_timeout_preserves_claim_and_python_work_without_proof(
        self,
    ):
        self.control(blocked=True, preload_wait_seconds=0.1)
        _, child = self.cluster_start()
        wait_for(lambda: self.events("load"))
        self.assertNotEqual(child.wait(timeout=5), 0)
        self.assertEqual(self.proofs(), [])
        self.assertEqual(len(self.claims()), 1)
        self.assertEqual(len(self.runtime_healths()), 1)
        self.assertEqual(self.events("release"), [])
        (self.home / "allow-load").touch()
        wait_for(lambda: self.events("release"))
        self.assertEqual(self.proofs(), [])
        self.assertEqual(len(self.claims()), 1)  # Recovery, not false completion.

    def test_cluster_rejected_preload_releases_claim_without_failed_proof(self):
        self.control(preload_reject=True)
        _, child = self.cluster_start()
        self.assertNotEqual(child.wait(timeout=15), 0)
        self.assertEqual(len(self.events("preload")), 1)
        self.assertEqual(self.events("load"), [])
        self.assertEqual(self.proofs(), [])
        self.assertEqual(self.claims(), [])
        self.assertEqual(len(self.runtime_healths()), 1)

    def test_cluster_stop_during_load_drains_then_releases_claim_without_next_route(
        self,
    ):
        self.cluster_definition(["chat", "embedding"])
        self.control(blocked=True)
        port, child = self.cluster_start()
        wait_for(lambda: self.events("load"))
        ref = request(port, "/healthz")[1]["server_ref"]
        stopper = self.spawn(local.CLI, "server", "stop", ref)
        time.sleep(0.2)
        self.assertIsNone(stopper.poll())
        self.assertEqual(len(self.claims()), 1)
        self.assertEqual(len(self.runtime_healths()), 1)
        (self.home / "allow-load").touch()
        self.assertEqual(stopper.wait(timeout=8), 0)
        child.wait(timeout=5)
        self.assertEqual(self.claims(), [])
        self.assertEqual(len(self.events("preload")), 1)
        self.assertEqual(len(self.events("release")), 1)

    def test_cluster_bind_failure_never_spawns_python_or_claims_on_either_host(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            port = listener.getsockname()[1]
            for binary in [local.CLI, local.DAEMON]:
                child = self.hidden_cluster(binary, port)
                self.assertNotEqual(child.wait(timeout=5), 0)
                self.assertEqual(self.events("spawn"), [])
                self.assertEqual(self.claims(), [])

    def test_cluster_real_drain_budget_retains_claim_when_python_is_still_loading(self):
        self.control(blocked=True)
        child = self.hidden_cluster(local.DAEMON, free_port())
        wait_for(lambda: self.events("load"))
        started = time.monotonic()
        child.terminate()
        self.assertNotEqual(child.wait(timeout=35), 0)
        self.assertGreaterEqual(time.monotonic() - started, 29)
        self.assertEqual(len(self.claims()), 1)
        self.assertEqual(self.proofs(), [])
        self.assertEqual(self.events("release"), [])
        self.assertEqual(len(self.runtime_healths()), 1)
        (self.home / "allow-load").touch()
        wait_for(lambda: self.events("release"))
        self.assertEqual(self.proofs(), [])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=local.CLI)
    parser.add_argument("--daemon", type=Path, default=local.DAEMON)
    args, tests = parser.parse_known_args()
    local.CLI, local.DAEMON = args.cli.resolve(), args.daemon.resolve()
    if os.name != "posix" or not local.CLI.is_file() or not local.DAEMON.is_file():
        parser.error("requires POSIX and built tentgent/tentgent-daemon binaries")
    loader = unittest.TestLoader()
    loader.testMethodPrefix = "test_cluster_"
    unittest.main(argv=[sys.argv[0], *tests], testLoader=loader, verbosity=2)
