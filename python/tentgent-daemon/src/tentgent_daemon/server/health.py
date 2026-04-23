from __future__ import annotations

from typing import Any

from .config import ServerConfig
from .session import RuntimeSession


def build_health_payload(config: ServerConfig, session: RuntimeSession) -> dict[str, Any]:
    snapshot = session.snapshot()
    return {
        "status": "ok",
        "server_ref": config.server_ref,
        "model_ref": config.model_ref,
        "host": config.host,
        "port": config.port,
        "lazy_load": config.lazy_load,
        "idle_seconds": config.idle_seconds,
        "runtime_home": str(config.home) if config.home is not None else None,
        "slice": "slice-6",
        "chat_ready": True,
        "model_loaded": snapshot.loaded,
        "startup_mode": snapshot.startup_mode,
        "idle_policy": snapshot.idle_policy,
        "last_activity_at": snapshot.last_activity_at,
        "last_release_at": snapshot.last_release_at,
        "last_release_reason": snapshot.last_release_reason,
    }
