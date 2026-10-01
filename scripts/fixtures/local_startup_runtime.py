"""Real Python runtime with instrumented fake load/chat backends; no downloads."""

from __future__ import annotations

import json
import os
import time
from contextlib import ExitStack
from pathlib import Path
from unittest.mock import patch

from starlette.responses import JSONResponse
from tentgent.runtime.backends.base import BackendFamily
from tentgent.runtime.backends.chat import ChatBackendModel, ChatResult
from tentgent.runtime.cli import daemon
from tentgent.runtime.server import preload
from tentgent.runtime.server.app import create_app

HOME_DIR = Path(os.environ["TENTGENT_HOME"])
CAPABILITY = "chat"
MODEL = None


def event(name):
    with (HOME_DIR / "backend-events.jsonl").open("a") as output:
        output.write(
            json.dumps(
                {
                    "event": name,
                    "pid": os.getpid(),
                    "capability": CAPABILITY,
                    "model_ref": MODEL,
                }
            )
            + "\n"
        )


class FixtureChatModel(ChatBackendModel):
    family = BackendFamily.TRANSFORMERS

    def __init__(self):
        self.loaded = False

    def load(self, record):
        event("load")
        control = json.loads((HOME_DIR / "control.json").read_text())
        deadline = time.monotonic() + 40
        blocked = (
            control.get("blocked")
            or control.get("block_capability") == CAPABILITY
            or control.get("block_model") == MODEL
        )
        while blocked and not (HOME_DIR / "allow-load").exists():
            if time.monotonic() > deadline:
                raise RuntimeError("fixture load gate was not opened")
            time.sleep(0.02)
        if (
            control.get("fail")
            or control.get("fail_capability") == CAPABILITY
            or control.get("fail_model") == MODEL
        ):
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
        control = json.loads((HOME_DIR / "control.json").read_text())
        return ChatResult(
            text=f"fixture {MODEL}"
            if control.get("include_model")
            else "fixture response"
        )

    def stream_generate(self, request):
        yield self.generate(request).text
        control = json.loads((HOME_DIR / "control.json").read_text())
        if control.get("block_stream"):
            event("stream_wait")
            deadline = time.monotonic() + 40
            while not (HOME_DIR / "allow-stream").exists():
                if time.monotonic() > deadline:
                    raise RuntimeError("fixture stream gate was not opened")
                time.sleep(0.02)
            yield " end"


def fixture_app(config, **kwargs):
    global CAPABILITY, MODEL
    CAPABILITY = config.capability.value
    MODEL = config.model_ref
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
            if control.get("preload_reject"):
                return JSONResponse(
                    status_code=501,
                    content={
                        "detail": {
                            "code": "preload_unsupported",
                            "message": "fixture admission rejection",
                        }
                    },
                )
        return await call_next(request)

    return app


if __name__ == "__main__":
    with ExitStack() as stack:
        for capability in [
            "chat",
            "embedding",
            "rerank",
            "audio_transcription",
            "vision_chat",
        ]:
            stack.enter_context(
                patch(
                    f"tentgent.runtime.server.app.build_{capability}_model",
                    lambda _: FixtureChatModel(),
                )
            )
        stack.enter_context(patch.object(daemon, "create_app", fixture_app))
        raise SystemExit(daemon.main())
