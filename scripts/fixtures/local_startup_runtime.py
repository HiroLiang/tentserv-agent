"""Real Python runtime with an instrumented fake chat backend; no model downloads."""

from __future__ import annotations

import json
import os
import time
from pathlib import Path
from unittest.mock import patch

from tentgent.runtime.backends.base import BackendFamily
from tentgent.runtime.backends.chat import ChatBackendModel, ChatResult
from tentgent.runtime.cli import daemon
from tentgent.runtime.server import preload
from tentgent.runtime.server.app import create_app

HOME_DIR = Path(os.environ["TENTGENT_HOME"])


def event(name):
    with (HOME_DIR / "backend-events.jsonl").open("a") as output:
        output.write(json.dumps({"event": name, "pid": os.getpid()}) + "\n")


class FixtureChatModel(ChatBackendModel):
    family = BackendFamily.TRANSFORMERS

    def __init__(self):
        self.loaded = False

    def load(self, record):
        event("load")
        control = json.loads((HOME_DIR / "control.json").read_text())
        deadline = time.monotonic() + 40
        while control.get("blocked") and not (HOME_DIR / "allow-load").exists():
            if time.monotonic() > deadline:
                raise RuntimeError("fixture load gate was not opened")
            time.sleep(0.02)
        if control.get("fail"):
            raise RuntimeError("injected backend load failure")
        self.loaded = True
        event("loaded")

    def release(self):
        self.loaded = False
        event("release")

    @property
    def is_loaded(self):
        return self.loaded

    def generate(self, request):
        assert self.is_loaded
        event("generate")
        return ChatResult(text="fixture response")

    def stream_generate(self, request):
        yield self.generate(request).text


def fixture_app(config, **kwargs):
    assert config.lazy_load, "managed Python launch must stay lazy"
    event("spawn")
    control = json.loads((HOME_DIR / "control.json").read_text())
    # Shorten only this fixture's wait for the timeout scenario, never production.
    preload.PRELOAD_WAIT_SECONDS = control.get("preload_wait_seconds", 300)
    app = create_app(config, **kwargs)

    @app.middleware("http")
    async def observe_preload(request, call_next):
        if request.url.path == "/v1/lifecycle/preload":
            event("preload")
        return await call_next(request)

    return app


if __name__ == "__main__":
    with (
        patch(
            "tentgent.runtime.server.app.build_chat_model", lambda _: FixtureChatModel()
        ),
        patch.object(daemon, "create_app", fixture_app),
    ):
        raise SystemExit(daemon.main())
