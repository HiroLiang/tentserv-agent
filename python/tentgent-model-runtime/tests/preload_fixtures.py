from __future__ import annotations

import asyncio
import json
from pathlib import Path
from threading import Event
from unittest.mock import patch

from tentgent.runtime.backends.base import BackendFamily, BackendModel
from tentgent.runtime.backends.records import ModelFormat, ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.server.app import create_app
from tentgent.runtime.server.lifecycle import RuntimeCapability, RuntimeServerConfig
from tentgent.runtime.server.managed_models import BoundModelContext
from tentgent.runtime.task.manager import TaskManager


class Clock:
    now = 0.0

    def __call__(self) -> float:
        return self.now


class FakeModel(BackendModel):
    family = BackendFamily.MLX

    def __init__(
        self, *, load_error=None, release_error=None, block=None, reports_loaded=True
    ):
        self.load_error = load_error
        self.release_error = release_error
        self.block = block
        self.reports_loaded = reports_loaded
        self.started = Event()
        self.loaded = False
        self.partial = False
        self.load_count = 0
        self.release_count = 0

    def load(self, record):
        self.load_count += 1
        self.partial = True
        self.started.set()
        if self.block is not None and not self.block.wait(timeout=5):
            raise AssertionError("test did not unblock load")
        self.loaded = self.reports_loaded
        if self.load_error is not None:
            raise self.load_error

    def release(self):
        self.release_count += 1
        if self.release_error is not None:
            raise self.release_error
        self.loaded = False
        self.partial = False

    @property
    def is_loaded(self):
        return self.loaded


def record(model_ref="model-ref", primary_format=ModelFormat.MLX):
    return ModelRecord(
        model_ref=model_ref,
        source_path=Path("/unused") / model_ref,
        primary_format=primary_format,
    )


def runtime(
    model=None,
    *,
    idle=0,
    capability=RuntimeCapability.CHAT,
    primary_format=ModelFormat.MLX,
    bound=True,
    token="generation-1",
    clock=None,
    max_workers=4,
):
    model = model or FakeModel()
    clock = clock or Clock()
    resources = ResourceManager(
        model_factory=lambda kind: model, model_idle_seconds=idle, clock=clock
    )
    tasks = TaskManager(clock=clock, max_workers=max_workers)
    context = (
        BoundModelContext(capability, record(primary_format=primary_format))
        if bound
        else None
    )
    with (
        patch("tentgent.runtime.server.app.bound_model_context", return_value=context),
        patch("tentgent.runtime.server.app._resource_manager", return_value=resources),
        patch("tentgent.runtime.server.app.TaskManager", return_value=tasks),
        patch.dict("os.environ", {"TENTGENT_RUNTIME_PROCESS_TOKEN": token}),
    ):
        app = create_app(
            RuntimeServerConfig(
                host="127.0.0.1", port=0, capability=capability, model_idle_seconds=idle
            )
        )
    return app, tasks, resources, model, clock


async def post(app, payload, *, raw=False):
    body = payload if raw else json.dumps(payload).encode()
    messages = []

    async def receive():
        return {"type": "http.request", "body": body, "more_body": False}

    async def send(message):
        messages.append(message)

    await app(
        {
            "type": "http",
            "http_version": "1.1",
            "method": "POST",
            "scheme": "http",
            "path": "/v1/lifecycle/preload",
            "raw_path": b"/v1/lifecycle/preload",
            "query_string": b"",
            "headers": [(b"content-type", b"application/json")],
            "client": ("127.0.0.1", 1234),
            "server": ("127.0.0.1", 8780),
        },
        receive,
        send,
    )
    status = next(
        message["status"]
        for message in messages
        if message["type"] == "http.response.start"
    )
    data = b"".join(
        message.get("body", b"")
        for message in messages
        if message["type"] == "http.response.body"
    )
    return status, json.loads(data)


def payload(task_ref="preload-1"):
    return {"task_ref": task_ref, "process_token": "generation-1"}


async def wait_event(event):
    assert await asyncio.to_thread(event.wait, 2), "test event was not reached"
