from concurrent.futures import ThreadPoolExecutor
from threading import Event

import pytest
from preload_fixtures import Clock, FakeModel, record
from tentgent.runtime.backends.resource_manager import (
    ModelResourceCleanupError,
    ModelResourceInvalidatedError,
    ResourceManager,
)


@pytest.mark.parametrize("idle", [0, 30])
@pytest.mark.parametrize("reports_loaded", [True, False])
def test_failed_load_cleans_partial_resource_and_fresh_retry(idle, reports_loaded):
    failed = FakeModel(
        load_error=RuntimeError("broken load") if reports_loaded else None,
        reports_loaded=reports_loaded,
    )
    fresh = FakeModel()
    models = iter([failed, fresh])
    manager = ResourceManager(
        model_factory=lambda kind: next(models), model_idle_seconds=idle
    )
    with pytest.raises(RuntimeError), manager.lease_model("chat", record()):
        pytest.fail("failed load must not yield")
    assert failed.release_count == 1
    assert not failed.partial
    assert manager.snapshot()["model_resource_count"] == 0
    with manager.lease_model("chat", record()) as model:
        assert model is fresh and model.is_loaded
    manager.release_all()


@pytest.mark.parametrize("fails", [False, True])
def test_reserved_waiter_serializes_or_rejects_invalidated_object(fails):
    release = Event()
    reserved = Event()
    model = FakeModel(
        block=release, load_error=RuntimeError("broken") if fails else None
    )

    class ObservedManager(ResourceManager):
        def _reserve_model_resource(self, *args, **kwargs):
            resource = super()._reserve_model_resource(*args, **kwargs)
            if resource.active_leases == 2:
                reserved.set()
            return resource

    manager = ObservedManager(model_factory=lambda kind: model, model_idle_seconds=30)

    def lease():
        with manager.lease_model("chat", record()) as loaded:
            assert loaded.is_loaded

    with ThreadPoolExecutor(max_workers=2) as pool:
        try:
            first = pool.submit(lease)
            assert model.started.wait(2)
            second = pool.submit(lease)
            assert reserved.wait(2)
            assert manager.snapshot()["model_resources"][0]["active_leases"] == 2
            assert manager.release_idle() == 0
        finally:
            release.set()
        if fails:
            with pytest.raises(RuntimeError, match="broken"):
                first.result(2)
            with pytest.raises(ModelResourceInvalidatedError):
                second.result(2)
            assert manager.snapshot()["model_resource_count"] == 0
            assert model.release_count == 1
        else:
            first.result(2)
            second.result(2)
            assert manager.snapshot()["model_resources"][0]["active_leases"] == 0
            assert model.release_count == 0
        assert model.load_count == 1
    manager.release_all()


@pytest.mark.parametrize("load_fails", [False, True])
def test_cleanup_failure_is_quarantined_not_reused_or_forgotten(load_fails):
    model = FakeModel(
        load_error=RuntimeError("broken") if load_fails else None,
        release_error=RuntimeError("cannot free"),
    )
    manager = ResourceManager(model_factory=lambda kind: model)
    with (
        pytest.raises(ModelResourceCleanupError, match="quarantined"),
        manager.lease_model("chat", record()),
    ):
        pass
    entry = manager.snapshot()["model_resources"][0]
    assert entry["state"] == "quarantined"
    assert entry["cleanup_error"] == "cannot free"
    assert entry["active_leases"] == 0
    assert manager.release_idle() == 0
    with (
        pytest.raises(ModelResourceCleanupError),
        manager.lease_model("chat", record()),
    ):
        pytest.fail("quarantined model must not be reused")
    assert model.load_count == model.release_count == 1
    model.release_error = None
    manager.release_all()
    assert model.release_count == 2 and not model.partial


def test_failed_loading_does_not_release_an_unrelated_active_or_retained_model():
    clock = Clock()
    good, bad = FakeModel(), FakeModel(load_error=RuntimeError("broken"))
    manager = ResourceManager(
        model_factory=lambda kind: good if kind == "good" else bad,
        model_idle_seconds=30,
        clock=clock,
    )
    with manager.lease_model("good", record("good")):
        clock.now = 100
        with pytest.raises(RuntimeError), manager.lease_model("bad", record("bad")):
            pass
        assert good.release_count == 0 and good.loaded
    assert good.release_count == 0 and good.loaded
    manager.release_all()


def test_inference_exception_does_not_invalidate_successfully_loaded_model():
    model = FakeModel()
    manager = ResourceManager(model_factory=lambda kind: model, model_idle_seconds=30)
    with (
        pytest.raises(ValueError, match="inference failed"),
        manager.lease_model("chat", record()),
    ):
        raise ValueError("inference failed")
    with manager.lease_model("chat", record()) as reused:
        assert reused is model
    assert model.load_count == 1 and model.release_count == 0
    assert manager.snapshot()["model_resources"][0]["state"] == "available"
    manager.release_all()


def test_release_all_attempts_other_resources_even_if_quarantine_cleanup_fails():
    bad, good = FakeModel(release_error=RuntimeError("cannot free")), FakeModel()
    manager = ResourceManager(
        model_factory=lambda kind: bad if kind == "bad" else good, model_idle_seconds=30
    )
    for kind in ("bad", "good"):
        with manager.lease_model(kind, record(kind)):
            pass
    with pytest.raises(ModelResourceCleanupError):
        manager.release_all()
    assert bad.release_count == good.release_count == 1
    assert not good.loaded
    assert manager.snapshot()["model_resource_count"] == 1
    assert manager.snapshot()["model_resources"][0]["state"] == "quarantined"
    bad.release_error = None
    manager.release_all()


def test_release_that_silently_keeps_model_loaded_is_quarantined():
    class BrokenRelease(FakeModel):
        def release(self):
            self.release_count += 1

    model = BrokenRelease()
    manager = ResourceManager(model_factory=lambda kind: model)
    with (
        pytest.raises(ModelResourceCleanupError, match="still loaded"),
        manager.lease_model("chat", record()),
    ):
        pass
    assert manager.snapshot()["model_resources"][0]["state"] == "quarantined"


@pytest.mark.parametrize("cleanup_fails", [False, True])
def test_lease_arriving_during_idle_release_waits_for_cleanup(cleanup_fails):
    releasing, unblock, reserved = Event(), Event(), Event()
    clock = Clock()

    class BlockingRelease(FakeModel):
        def release(self):
            releasing.set()
            assert unblock.wait(5)
            super().release()

    class ObservedManager(ResourceManager):
        def _reserve_model_resource(self, *args, **kwargs):
            resource = super()._reserve_model_resource(*args, **kwargs)
            if resource.active_leases == 2:
                reserved.set()
            return resource

    model = BlockingRelease(
        release_error=RuntimeError("cannot free") if cleanup_fails else None
    )
    manager = ObservedManager(
        model_factory=lambda kind: model, model_idle_seconds=1, clock=clock
    )
    with manager.lease_model("chat", record()):
        pass
    clock.now = 1

    def lease():
        with manager.lease_model("chat", record()) as loaded:
            assert loaded.is_loaded

    with ThreadPoolExecutor(max_workers=2) as pool:
        try:
            cleanup = pool.submit(manager.release_idle)
            assert releasing.wait(2)
            waiter = pool.submit(lease)
            assert reserved.wait(2)
            assert model.load_count == 1
        finally:
            unblock.set()
        if cleanup_fails:
            for future in (cleanup, waiter):
                with pytest.raises(ModelResourceCleanupError):
                    future.result(2)
            assert model.load_count == 1
            assert manager.snapshot()["model_resources"][0]["state"] == "quarantined"
        else:
            assert cleanup.result(2) == 1
            waiter.result(2)
            assert model.load_count == 2
            assert manager.snapshot()["model_resources"][0]["state"] == "available"
        assert manager.snapshot()["model_resource_count"] == 1
        assert manager.snapshot()["model_resources"][0]["active_leases"] == 0
    model.release_error = None
    manager.release_all()
