import asyncio

import pytest
from preload_fixtures import FakeModel, payload, post, runtime
from tentgent.runtime.backends.records import ModelFormat
from tentgent.runtime.server.lifecycle import RuntimeCapability


@pytest.mark.parametrize("idle", [0, 30])
def test_preload_response_is_bound_and_finishes_after_lease_exit(idle):
    async def run():
        app, tasks, resources, model, clock = runtime(idle=idle)
        try:
            status, body = await post(app, payload())
            assert status == 200
            assert body == {
                "status": "done",
                "task_ref": "preload-1",
                "model_ref": "model-ref",
                "capability": "chat",
                "process_token": "generation-1",
            }
            assert model.load_count == 1
            assert not tasks.has_active_tasks()
            assert resources.snapshot()["model_resource_count"] == (1 if idle else 0)
            if idle:
                assert resources.snapshot()["model_resources"][0]["active_leases"] == 0
                assert (await post(app, payload("preload-2")))[0] == 200
                assert model.load_count == 1
                clock.now = 29
                assert resources.release_idle() == 0
                clock.now = 30
                assert resources.release_idle() == 1
            assert model.release_count == 1 and not model.loaded
        finally:
            tasks.shutdown()
            resources.release_all()

    asyncio.run(run())


@pytest.mark.parametrize(
    "invalid",
    [
        None,
        [],
        {},
        {"task_ref": "t"},
        {"process_token": "generation-1"},
        {"task_ref": None, "process_token": "generation-1"},
        {"task_ref": "t", "process_token": None},
        {"task_ref": 1, "process_token": "generation-1"},
        {"task_ref": "t", "process_token": False},
        {"task_ref": " ", "process_token": "generation-1"},
        {"task_ref": "t", "process_token": ""},
        {**payload(), "model": {"source_path": "/arbitrary"}},
        {**payload(), "capability": "chat"},
        {**payload(), "backend": "mlx"},
        {**payload(), "workflow": "text-to-image"},
        {**payload(), "adapter_ref": "adapter"},
    ],
)
def test_invalid_preload_payload_is_rejected_before_any_work(invalid):
    async def run():
        app, tasks, resources, model, _ = runtime()
        try:
            status, body = await post(app, invalid)
            assert status == 422
            assert body["detail"]["code"] == "invalid_preload_request"
            assert (
                model.load_count == 0
                and resources.snapshot()["model_resource_count"] == 0
            )
            assert tasks.snapshot()["task_count"] == 0
        finally:
            tasks.shutdown()

    asyncio.run(run())


def test_malformed_json_uses_the_same_error_envelope():
    async def run():
        app, tasks, _, _, _ = runtime()
        try:
            status, body = await post(app, b'{"task_ref":', raw=True)
            assert status == 422 and body["detail"]["code"] == "invalid_preload_request"
        finally:
            tasks.shutdown()

    asyncio.run(run())


@pytest.mark.parametrize(
    "case,expected_code",
    [
        ("wrong-token", "runtime_generation_mismatch"),
        ("missing-runtime-token", "runtime_generation_mismatch"),
        ("closing", "runtime_closing"),
        ("shutdown", "runtime_closing"),
        ("duplicate", "preload_task_exists"),
    ],
)
def test_conflicts_do_not_submit_or_leak_generation(case, expected_code):
    async def run():
        app, tasks, resources, model, _ = runtime(
            token="" if case == "missing-runtime-token" else "generation-1"
        )
        request = payload()
        try:
            if case == "wrong-token":
                request["process_token"] = "stale-generation"
            elif case == "closing":
                tasks.begin_closing()
            elif case == "shutdown":
                tasks.mark_shutdown()
            elif case == "duplicate":
                assert (await post(app, request))[0] == 200
            status, body = await post(app, request)
            assert status == 409 and body["detail"]["code"] == expected_code
            assert "generation-1" not in str(body)
            assert model.load_count == (1 if case == "duplicate" else 0)
            assert resources.snapshot()["model_resource_count"] == 0
        finally:
            tasks.shutdown()

    asyncio.run(run())


@pytest.mark.parametrize(
    "capability,format",
    [
        (RuntimeCapability.IMAGE_GENERATION, ModelFormat.DIFFUSERS),
        (RuntimeCapability.IMAGE_GENERATION, ModelFormat.MLX),
        (RuntimeCapability.LORA_TUNING, ModelFormat.MLX),
        (RuntimeCapability.EMBEDDING, ModelFormat.MLX),
        (RuntimeCapability.RERANK, ModelFormat.MLX),
        (RuntimeCapability.CHAT, ModelFormat.DIFFUSERS),
    ],
)
def test_unsupported_preload_cannot_claim_metadata_only_success(capability, format):
    async def run():
        app, tasks, _, model, _ = runtime(capability=capability, primary_format=format)
        try:
            status, body = await post(app, payload())
            assert status == 501 and body["detail"]["code"] == "preload_unsupported"
            assert tasks.snapshot()["task_count"] == 0 and model.load_count == 0
        finally:
            tasks.shutdown()

    asyncio.run(run())


def test_unbound_runtime_is_not_allowed_to_select_an_arbitrary_model():
    async def run():
        app, tasks, _, model, _ = runtime(bound=False)
        try:
            status, body = await post(app, payload())
            assert status == 400 and body["detail"]["code"] == "preload_unbound_runtime"
            assert model.load_count == 0
        finally:
            tasks.shutdown()

    asyncio.run(run())


@pytest.mark.parametrize(
    "error,status",
    [
        (RuntimeError("broken weights"), 500),
        (TimeoutError("backend load timed out"), 500),
        (NotImplementedError("unsupported loader"), 501),
        (ModuleNotFoundError("backend dependency missing"), 501),
        (
            RuntimeError(
                "Python model runtime dependency is not installed. Missing Python package: mlx."
            ),
            501,
        ),
    ],
)
def test_load_errors_return_accepted_task_identity_and_cleanup(error, status):
    async def run():
        app, tasks, resources, model, _ = runtime(FakeModel(load_error=error), idle=30)
        try:
            actual, body = await post(app, payload())
            assert actual == status
            assert body["detail"]["task_ref"] == "preload-1"
            assert body["detail"]["message"] == str(error)
            assert body["detail"]["code"] == (
                "preload_unsupported" if status == 501 else "preload_failed"
            )
            assert model.release_count == 1 and not model.partial
            assert resources.snapshot()["model_resource_count"] == 0
        finally:
            tasks.shutdown()

    asyncio.run(run())


def test_cleanup_failure_returns_500_and_keeps_diagnostic_quarantine():
    async def run():
        model = FakeModel(
            load_error=ModuleNotFoundError("missing dependency"),
            release_error=RuntimeError("cleanup failed"),
        )
        app, tasks, resources, _, _ = runtime(model, idle=30)
        try:
            status, body = await post(app, payload())
            assert status == 500 and body["detail"]["code"] == "preload_failed"
            assert body["detail"]["task_ref"] == "preload-1"
            assert resources.snapshot()["model_resources"][0]["state"] == "quarantined"
        finally:
            tasks.shutdown()
            model.release_error = None
            resources.release_all()

    asyncio.run(run())
