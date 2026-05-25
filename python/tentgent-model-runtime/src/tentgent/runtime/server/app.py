from __future__ import annotations

from collections.abc import AsyncIterator, Callable
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, HTTPException

from tentgent.runtime import __version__
from tentgent.runtime.backends.audio_speech import (
    AudioSpeechBackendModel,
    build_audio_speech_model,
)
from tentgent.runtime.backends.audio_transcription import (
    AudioTranscriptionBackendModel,
    build_audio_transcription_model,
)
from tentgent.runtime.backends.chat import ChatBackendModel, build_chat_model
from tentgent.runtime.backends.embedding import (
    EmbeddingBackendModel,
    build_embedding_model,
)
from tentgent.runtime.backends.image_generation import (
    ImageGenerationBackendModel,
    build_image_generation_model,
)
from tentgent.runtime.backends.rerank import RerankBackendModel, build_rerank_model
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.backends.video_understanding import (
    VideoUnderstandingBackendModel,
    build_video_understanding_model,
)
from tentgent.runtime.backends.vision_chat import (
    VisionChatBackendModel,
    build_vision_chat_model,
)
from tentgent.runtime.server.lifecycle import (
    RuntimeCapability,
    RuntimeLifecycleState,
    RuntimeServerConfig,
)
from tentgent.runtime.server.routes import (
    audio_speech,
    audio_transcription,
    chat,
    embedding,
    health,
    image_generation,
    lifecycle,
    rerank,
    video_understanding,
    vision_chat,
)
from tentgent.runtime.task.manager import TaskManager


def create_app(
    config: RuntimeServerConfig,
    *,
    request_shutdown: Callable[[], None] | None = None,
) -> FastAPI:
    task_manager = TaskManager()
    resource_manager = _resource_manager(config)
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
    app.include_router(lifecycle.router)
    _include_capability_router(app, config.capability)
    _include_unsupported_capability_routes(app, config.capability)
    return app


def _resource_manager(config: RuntimeServerConfig) -> ResourceManager[Any]:
    if config.capability == RuntimeCapability.AUDIO_TRANSCRIPTION:
        audio_transcription_resources: ResourceManager[
            AudioTranscriptionBackendModel
        ] = ResourceManager(
            model_factory=build_audio_transcription_model,
            model_idle_timeout_seconds=config.model_idle_timeout_seconds,
        )
        return audio_transcription_resources
    if config.capability == RuntimeCapability.AUDIO_SPEECH:
        audio_speech_resources: ResourceManager[AudioSpeechBackendModel] = (
            ResourceManager(
                model_factory=build_audio_speech_model,
                model_idle_timeout_seconds=config.model_idle_timeout_seconds,
            )
        )
        return audio_speech_resources
    if config.capability == RuntimeCapability.CHAT:
        chat_resources: ResourceManager[ChatBackendModel] = ResourceManager(
            model_factory=build_chat_model,
            model_idle_timeout_seconds=config.model_idle_timeout_seconds,
        )
        return chat_resources
    if config.capability == RuntimeCapability.EMBEDDING:
        embedding_resources: ResourceManager[EmbeddingBackendModel] = ResourceManager(
            model_factory=build_embedding_model,
            model_idle_timeout_seconds=config.model_idle_timeout_seconds,
        )
        return embedding_resources
    if config.capability == RuntimeCapability.IMAGE_GENERATION:
        image_generation_resources: ResourceManager[ImageGenerationBackendModel] = (
            ResourceManager(
                model_factory=build_image_generation_model,
                model_idle_timeout_seconds=config.model_idle_timeout_seconds,
            )
        )
        return image_generation_resources
    if config.capability == RuntimeCapability.RERANK:
        rerank_resources: ResourceManager[RerankBackendModel] = ResourceManager(
            model_factory=build_rerank_model,
            model_idle_timeout_seconds=config.model_idle_timeout_seconds,
        )
        return rerank_resources
    if config.capability == RuntimeCapability.VIDEO_UNDERSTANDING:
        video_understanding_resources: ResourceManager[
            VideoUnderstandingBackendModel
        ] = ResourceManager(
            model_factory=build_video_understanding_model,
            model_idle_timeout_seconds=config.model_idle_timeout_seconds,
        )
        return video_understanding_resources
    if config.capability == RuntimeCapability.VISION_CHAT:
        vision_chat_resources: ResourceManager[VisionChatBackendModel] = ResourceManager(
            model_factory=build_vision_chat_model,
            model_idle_timeout_seconds=config.model_idle_timeout_seconds,
        )
        return vision_chat_resources

    raise ValueError(f"unsupported runtime capability `{config.capability}`")


def _include_capability_router(app: FastAPI, capability: RuntimeCapability) -> None:
    if capability == RuntimeCapability.AUDIO_TRANSCRIPTION:
        app.include_router(audio_transcription.router)
    elif capability == RuntimeCapability.AUDIO_SPEECH:
        app.include_router(audio_speech.router)
    elif capability == RuntimeCapability.CHAT:
        app.include_router(chat.router)
    elif capability == RuntimeCapability.EMBEDDING:
        app.include_router(embedding.router)
    elif capability == RuntimeCapability.IMAGE_GENERATION:
        app.include_router(image_generation.router)
    elif capability == RuntimeCapability.RERANK:
        app.include_router(rerank.router)
    elif capability == RuntimeCapability.VIDEO_UNDERSTANDING:
        app.include_router(video_understanding.router)
    elif capability == RuntimeCapability.VISION_CHAT:
        app.include_router(vision_chat.router)
    else:
        raise ValueError(f"unsupported runtime capability `{capability}`")


def _include_unsupported_capability_routes(
    app: FastAPI,
    capability: RuntimeCapability,
) -> None:
    targets = {
        RuntimeCapability.AUDIO_TRANSCRIPTION: ("/v1/audio/transcriptions",),
        RuntimeCapability.AUDIO_SPEECH: ("/v1/audio/speech",),
        RuntimeCapability.CHAT: ("/v1/chat", "/v1/chat/stream"),
        RuntimeCapability.EMBEDDING: ("/v1/embeddings",),
        RuntimeCapability.IMAGE_GENERATION: (
            "/v1/images/generations",
            "/v1/images/transforms",
            "/v1/images/inpaint",
            "/v1/images/control",
        ),
        RuntimeCapability.RERANK: ("/v1/rerank",),
        RuntimeCapability.VIDEO_UNDERSTANDING: ("/v1/video/understanding",),
        RuntimeCapability.VISION_CHAT: ("/v1/vision/chat",),
    }
    for target_capability, paths in targets.items():
        if target_capability == capability:
            continue
        for path in paths:
            _add_unsupported_route(app, path, capability)


def _add_unsupported_route(
    app: FastAPI,
    path: str,
    capability: RuntimeCapability,
) -> None:
    async def unsupported_target() -> None:
        raise HTTPException(
            status_code=400,
            detail=(
                f"runtime capability `{capability.value}` does not serve "
                f"`POST {path}`"
            ),
        )

    app.add_api_route(
        path,
        unsupported_target,
        methods=["POST"],
        name=f"unsupported_{capability.value}_{path.replace('/', '_')}",
    )
