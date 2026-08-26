from __future__ import annotations

import asyncio
import unittest
from pathlib import Path
from threading import Event
from types import SimpleNamespace
from unittest.mock import patch

from tentgent.runtime.backends.base import BackendFamily, BackendModel
from tentgent.runtime.backends.mlx.chat import MlxChatModel
from tentgent.runtime.backends.records import ModelFormat, ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.cli.daemon import parse_args
from tentgent.runtime.server.lifecycle import RuntimeLifecycleState, RuntimeServerConfig
from tentgent.runtime.server.routes.health import healthz
from tentgent.runtime.task.manager import TaskManager
from tentgent.runtime.task.task import ExecutionTarget, RuntimeTask, TaskKind


class _Clock:
    def __init__(self) -> None:
        self.now = 0.0

    def __call__(self) -> float:
        return self.now


class _FakeModel(BackendModel):
    family = BackendFamily.MLX

    def __init__(self) -> None:
        self.loaded = False
        self.load_count = 0
        self.release_count = 0

    def load(self, record: ModelRecord) -> None:
        del record
        self.loaded = True
        self.load_count += 1

    def release(self) -> None:
        self.loaded = False
        self.release_count += 1

    @property
    def is_loaded(self) -> bool:
        return self.loaded


class _ImmediateTask(RuntimeTask[object, str]):
    def __init__(self, task_ref: str = "task-1") -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.CHAT,
            request=object(),
            execution_target=ExecutionTarget.LOCAL,
        )

    def execute(self) -> str:
        return "done"


class _BlockingTask(RuntimeTask[object, str]):
    def __init__(self, started: Event, release: Event) -> None:
        super().__init__(
            task_ref="blocking-task",
            kind=TaskKind.CHAT,
            request=object(),
            execution_target=ExecutionTarget.LOCAL,
        )
        self._started = started
        self._release = release

    def execute(self) -> str:
        self._started.set()
        self._release.wait(timeout=1)
        return "done"


class RuntimeDaemonArgumentTests(unittest.TestCase):
    def test_canonical_defaults_are_finite(self) -> None:
        args = parse_args(["--port", "8780"])

        self.assertEqual(args.runtime_idle_seconds, 300.0)
        self.assertEqual(args.model_idle_seconds, 0.0)

    def test_deprecated_aliases_remain_compatible(self) -> None:
        args = parse_args(
            [
                "--port",
                "8780",
                "--idle-keep-alive-seconds",
                "30",
                "--model-idle-timeout-seconds",
                "5",
            ]
        )

        self.assertEqual(args.runtime_idle_seconds, 30.0)
        self.assertEqual(args.model_idle_seconds, 5.0)

    def test_matching_canonical_and_deprecated_values_are_valid(self) -> None:
        args = parse_args(
            [
                "--port",
                "8780",
                "--runtime-idle-seconds",
                "30",
                "--idle-keep-alive-seconds",
                "30",
            ]
        )

        self.assertEqual(args.runtime_idle_seconds, 30.0)

    def test_conflicting_aliases_are_rejected(self) -> None:
        with self.assertRaises(SystemExit):
            parse_args(
                [
                    "--port",
                    "8780",
                    "--runtime-idle-seconds",
                    "30",
                    "--idle-keep-alive-seconds",
                    "31",
                ]
            )

    def test_invalid_timeout_pairs_are_rejected(self) -> None:
        invalid_arguments = (
            ["--port", "8780", "--runtime-idle-seconds", "-1"],
            ["--port", "8780", "--model-idle-seconds", "-1"],
            ["--port", "8780", "--runtime-idle-seconds", "nan"],
            [
                "--port",
                "8780",
                "--runtime-idle-seconds",
                "5",
                "--model-idle-seconds",
                "6",
            ],
        )

        for arguments in invalid_arguments:
            with self.subTest(arguments=arguments), self.assertRaises(SystemExit):
                parse_args(arguments)


class ResourceManagerLifecycleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.record = ModelRecord(
            model_ref="a" * 64,
            source_path=Path("/tmp/model"),
            primary_format=ModelFormat.MLX,
        )

    def test_zero_timeout_releases_and_later_lease_reloads(self) -> None:
        models: list[_FakeModel] = []

        def factory(kind: object) -> _FakeModel:
            del kind
            model = _FakeModel()
            models.append(model)
            return model

        manager = ResourceManager(
            model_factory=factory,
            model_idle_seconds=0,
        )

        with manager.lease_model("chat", self.record) as first:
            self.assertTrue(first.is_loaded)
            self.assertEqual(manager.snapshot()["model_resource_count"], 1)

        self.assertEqual(models[0].release_count, 1)
        self.assertEqual(manager.snapshot()["model_resource_count"], 0)

        with manager.lease_model("chat", self.record) as second:
            self.assertIsNot(second, first)
            self.assertTrue(second.is_loaded)

        self.assertEqual(len(models), 2)
        self.assertEqual(models[1].release_count, 1)

    def test_positive_timeout_waits_for_final_lease_and_expiry(self) -> None:
        clock = _Clock()
        model = _FakeModel()
        manager = ResourceManager(
            model_factory=lambda kind: model,
            model_idle_seconds=5,
            clock=clock,
        )

        with manager.lease_model("chat", self.record):
            clock.now = 100
            self.assertEqual(manager.release_idle(), 0)

        clock.now = 104.999
        self.assertEqual(manager.release_idle(), 0)
        clock.now = 105
        self.assertEqual(manager.release_idle(), 1)
        self.assertEqual(model.release_count, 1)

    def test_negative_or_non_finite_timeout_is_rejected(self) -> None:
        for timeout in (-1.0, float("inf"), float("nan")):
            with self.subTest(timeout=timeout), self.assertRaises(ValueError):
                ResourceManager(
                    model_factory=lambda kind: _FakeModel(),
                    model_idle_seconds=timeout,
                )


class MlxReleaseTests(unittest.TestCase):
    def test_chat_release_drops_all_owned_references_and_clears_cache(self) -> None:
        model = MlxChatModel()
        model._record = object()  # type: ignore[assignment]
        model._load_path = "/tmp/model"
        model._model = object()
        model._tokenizer = object()
        model._active_adapter_ref = "adapter-ref"

        with patch("tentgent.runtime.backends.mlx.chat.clear_mlx_cache") as clear_cache:
            model.release()

        self.assertIsNone(model._record)
        self.assertIsNone(model._load_path)
        self.assertIsNone(model._model)
        self.assertIsNone(model._tokenizer)
        self.assertIsNone(model._active_adapter_ref)
        clear_cache.assert_called_once_with()


class TaskActivityTests(unittest.TestCase):
    def test_active_task_blocks_idle_and_completion_reanchors_clock(self) -> None:
        clock = _Clock()
        started = Event()
        release = Event()
        manager = TaskManager(max_workers=1, clock=clock)
        handle = manager.submit(_BlockingTask(started, release))
        self.assertTrue(started.wait(timeout=1))

        clock.now = 100
        self.assertFalse(manager.is_idle_for(30))
        release.set()
        self.assertEqual(handle.future.result(timeout=1), "done")
        clock.now = 129.999
        self.assertFalse(manager.is_idle_for(30))
        clock.now = 130
        self.assertTrue(manager.is_idle_for(30))
        manager.shutdown()

    def test_retained_terminal_metadata_does_not_postpone_runtime_idle(self) -> None:
        clock = _Clock()
        manager = TaskManager(
            max_workers=1,
            completed_retention_seconds=60,
            clock=clock,
        )
        handle = manager.submit(_ImmediateTask())
        self.assertEqual(handle.future.result(timeout=1), "done")

        clock.now = 5
        self.assertFalse(manager.has_active_tasks())
        self.assertEqual(manager.snapshot()["task_count"], 1)
        self.assertTrue(manager.is_idle_for(5))
        manager.shutdown()

    def test_health_is_observational(self) -> None:
        class _ObservedLifecycle:
            def snapshot(self) -> dict[str, str]:
                return {"status": "ok"}

        class _ExplodingTaskManager:
            def touch_activity(self) -> None:
                raise AssertionError("health must not mutate task activity")

        request = SimpleNamespace(
            app=SimpleNamespace(
                state=SimpleNamespace(
                    lifecycle=_ObservedLifecycle(),
                    task_manager=_ExplodingTaskManager(),
                )
            )
        )

        self.assertEqual(healthz(request), {"status": "ok"})


class RuntimeLifecycleWatcherTests(unittest.IsolatedAsyncioTestCase):
    async def test_runtime_stop_releases_all_retained_models(self) -> None:
        model = _FakeModel()
        manager = TaskManager()
        resources = ResourceManager(
            model_factory=lambda kind: model,
            model_idle_seconds=30,
        )
        with resources.lease_model(
            "chat",
            ModelRecord(
                model_ref="a" * 64,
                source_path=Path("/tmp/model"),
                primary_format=ModelFormat.MLX,
            ),
        ):
            pass
        lifecycle = RuntimeLifecycleState(
            config=RuntimeServerConfig(
                host="127.0.0.1",
                port=0,
                runtime_idle_seconds=60,
                model_idle_seconds=30,
                task_poll_interval_seconds=60,
            ),
            task_manager=manager,
            resource_manager=resources,
        )

        await lifecycle.start()
        self.assertEqual(resources.snapshot()["model_resource_count"], 1)
        await lifecycle.stop()
        self.assertEqual(resources.snapshot()["model_resource_count"], 0)
        self.assertEqual(model.release_count, 1)

    async def test_runtime_idle_clock_is_reset_when_startup_becomes_ready(self) -> None:
        clock = _Clock()
        manager = TaskManager(clock=clock)
        resources = ResourceManager(model_factory=lambda kind: _FakeModel())
        lifecycle = RuntimeLifecycleState(
            config=RuntimeServerConfig(
                host="127.0.0.1",
                port=0,
                runtime_idle_seconds=30,
                model_idle_seconds=0,
                task_poll_interval_seconds=60,
            ),
            task_manager=manager,
            resource_manager=resources,
        )

        clock.now = 100
        await lifecycle.start()
        self.assertFalse(manager.is_idle_for(30))
        clock.now = 130
        self.assertTrue(manager.is_idle_for(30))
        await lifecycle.stop()

    async def test_runtime_exits_from_initial_ready_idle_clock(self) -> None:
        shutdown_requested = asyncio.Event()
        manager = TaskManager()
        resources = ResourceManager(model_factory=lambda kind: _FakeModel())
        lifecycle = RuntimeLifecycleState(
            config=RuntimeServerConfig(
                host="127.0.0.1",
                port=0,
                runtime_idle_seconds=0.01,
                model_idle_seconds=0,
                closing_grace_seconds=0,
                task_poll_interval_seconds=0.005,
            ),
            task_manager=manager,
            resource_manager=resources,
            request_shutdown=shutdown_requested.set,
        )

        await lifecycle.start()
        await asyncio.wait_for(shutdown_requested.wait(), timeout=1)
        self.assertEqual(lifecycle.snapshot()["status"], "shutdown")
        await lifecycle.stop()


if __name__ == "__main__":
    unittest.main()
