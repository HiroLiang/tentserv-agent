import asyncio
from threading import Event
from unittest.mock import patch

import pytest
from preload_fixtures import FakeModel, payload, post, runtime, wait_event
from tentgent.runtime.server.lifecycle import RuntimeLifecycleState, RuntimeServerConfig
from tentgent.runtime.task.task import RuntimeTask, TaskKind


class BlockingTask(RuntimeTask[object, None]):
    def __init__(self, unblock):
        super().__init__(task_ref="occupy-worker", kind=TaskKind.CHAT, request=None)
        self.unblock = unblock
        self.started = Event()

    def execute(self):
        self.started.set()
        assert self.unblock.wait(5)


@pytest.mark.parametrize("queued", [False, True])
@pytest.mark.parametrize("abandon", ["timeout", "cancel"])
@pytest.mark.parametrize("fails", [False, True])
@pytest.mark.parametrize("idle", [0, 30])
def test_abandoned_wait_keeps_queued_or_running_work_active(
    queued, abandon, fails, idle
):
    async def run():
        load_release, worker_release = Event(), Event()
        model = FakeModel(
            block=load_release,
            load_error=RuntimeError("late load failure") if fails else None,
        )
        app, tasks, resources, _, clock = runtime(model, max_workers=1, idle=idle)
        submitted = asyncio.Event()
        handles = []
        original_submit = tasks.submit

        def capture_submit(task):
            handle = original_submit(task)
            handles.append(handle)
            submitted.set()
            return handle

        loop = asyncio.get_running_loop()
        unhandled = []
        loop.set_exception_handler(lambda loop, context: unhandled.append(context))
        try:
            if queued:
                blocker = BlockingTask(worker_release)
                original_submit(blocker)
                await wait_event(blocker.started)
            with (
                patch.object(tasks, "submit", side_effect=capture_submit),
                patch(
                    "tentgent.runtime.server.preload.PRELOAD_WAIT_SECONDS",
                    0.01 if abandon == "timeout" else 300,
                ),
            ):
                waiter = asyncio.create_task(post(app, payload()))
                await asyncio.wait_for(submitted.wait(), 2)
                if not queued:
                    await wait_event(model.started)
                if abandon == "cancel":
                    waiter.cancel()
                    with pytest.raises(asyncio.CancelledError):
                        await waiter
                else:
                    status, body = await waiter
                    assert status == 504
                    assert body["detail"]["code"] == "preload_wait_timeout"
                    assert body["detail"]["task_ref"] == "preload-1"
            handle = handles[0]
            assert handle.task.kind == TaskKind.PRELOAD
            assert not handle.future.cancelled() and not handle.future.done()
            clock.now = 1000
            assert tasks.has_active_tasks() and not tasks.is_idle_for(300)
            if queued:
                assert resources.snapshot()["model_resource_count"] == 0
                assert handle.task.status.value == "pending"
                worker_release.set()
                await wait_event(model.started)
            assert resources.snapshot()["model_resources"][0]["active_leases"] == 1
            assert resources.release_idle() == 0
            tasks.begin_closing()
            load_release.set()
            if fails:
                with pytest.raises(RuntimeError, match="late load failure"):
                    await asyncio.wrap_future(handle.future)
            else:
                await asyncio.wrap_future(handle.future)
            assert not tasks.has_active_tasks()
            assert resources.snapshot()["model_resource_count"] == (
                1 if idle and not fails else 0
            )
            if idle and not fails:
                assert resources.snapshot()["model_resources"][0]["active_leases"] == 0
            else:
                assert model.release_count == 1 and not model.partial
            assert not tasks.is_idle_for(300)
            clock.now = 1299.9
            assert not tasks.is_idle_for(300)
            clock.now = 1300
            assert tasks.is_idle_for(300)
            await asyncio.sleep(0)  # Deliver the abandoned wait's completion callback.
            assert not unhandled
        finally:
            worker_release.set()
            load_release.set()
            await asyncio.gather(
                *(asyncio.wrap_future(handle.future) for handle in handles),
                return_exceptions=True,
            )
            tasks.shutdown()
            resources.release_all()

    asyncio.run(run())


def test_preloads_of_one_resource_serialize_and_reuse_a_real_load():
    async def run():
        unblock = Event()
        model = FakeModel(block=unblock)
        app, tasks, resources, _, _ = runtime(model, idle=30)
        reserved = asyncio.Event()
        original_reserve = resources._reserve_model_resource
        loop = asyncio.get_running_loop()

        def observe(*args, **kwargs):
            resource = original_reserve(*args, **kwargs)
            if resource.active_leases == 2:
                loop.call_soon_threadsafe(reserved.set)
            return resource

        waiters = []
        try:
            with patch.object(
                resources, "_reserve_model_resource", side_effect=observe
            ):
                waiters.append(asyncio.create_task(post(app, payload("one"))))
                await wait_event(model.started)
                waiters.append(asyncio.create_task(post(app, payload("two"))))
                await asyncio.wait_for(reserved.wait(), 2)
                assert model.load_count == 1
                unblock.set()
                assert [response[0] for response in await asyncio.gather(*waiters)] == [
                    200,
                    200,
                ]
            assert model.load_count == 1 and model.release_count == 0
            assert not tasks.has_active_tasks()
        finally:
            unblock.set()
            await asyncio.gather(*waiters, return_exceptions=True)
            tasks.shutdown()
            resources.release_all()

    asyncio.run(run())


def test_idle_cleanup_failure_does_not_disable_runtime_shutdown(caplog):
    async def run():
        model = FakeModel(release_error=RuntimeError("cleanup failed"))
        app, tasks, resources, _, clock = runtime(model, idle=1)
        shutdown = asyncio.Event()
        lifecycle = RuntimeLifecycleState(
            config=RuntimeServerConfig(
                host="127.0.0.1",
                port=0,
                runtime_idle_seconds=2,
                model_idle_seconds=1,
                task_poll_interval_seconds=0.001,
                closing_grace_seconds=0,
            ),
            task_manager=tasks,
            resource_manager=resources,
            request_shutdown=shutdown.set,
        )
        try:
            assert (await post(app, payload()))[0] == 200
            await lifecycle.start()
            clock.now = 2
            await asyncio.wait_for(shutdown.wait(), 2)
            assert resources.snapshot()["model_resources"][0]["state"] == "quarantined"
            assert "cleanup failed" in caplog.text
            assert tasks.state.value == "shutdown"
        finally:
            model.release_error = None
            await lifecycle.stop()
        assert model.release_count == 2 and not model.partial

    asyncio.run(run())
