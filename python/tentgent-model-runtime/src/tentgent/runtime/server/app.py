from __future__ import annotations

from collections.abc import AsyncIterator, Callable
from contextlib import asynccontextmanager

from fastapi import FastAPI

from tentgent.runtime import __version__
from tentgent.runtime.backends.chat import ChatBackendModel, build_chat_model
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.server.lifecycle import RuntimeLifecycleState, RuntimeServerConfig
from tentgent.runtime.server.routes import chat, health
from tentgent.runtime.task.manager import TaskManager


def create_app(
    config: RuntimeServerConfig,
    *,
    request_shutdown: Callable[[], None] | None = None,
) -> FastAPI:
    task_manager = TaskManager()
    resource_manager: ResourceManager[ChatBackendModel] = ResourceManager(
        model_factory=build_chat_model,
        model_idle_timeout_seconds=config.model_idle_timeout_seconds,
    )
    lifecycle_state = RuntimeLifecycleState(
        config=config,
        task_manager=task_manager,
        resource_manager=resource_manager,
        request_shutdown=request_shutdown,
    )

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        await lifecycle_state.start()
        try:
            yield
        finally:
            await lifecycle_state.stop()

    app = FastAPI(
        title="Tentgent Model Runtime",
        version=__version__,
        docs_url=None,
        redoc_url=None,
        lifespan=lifespan,
    )
    app.state.lifecycle = lifecycle_state
    app.state.task_manager = task_manager
    app.state.resource_manager = resource_manager
    app.include_router(health.router)
    app.include_router(chat.router)
    return app
