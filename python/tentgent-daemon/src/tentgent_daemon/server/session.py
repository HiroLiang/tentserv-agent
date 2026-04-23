from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from threading import Lock
from time import monotonic

from tentgent_daemon.backends import ChatBackend, create_backend
from tentgent_daemon.runtime.chat import ChatRequest, Message
from tentgent_daemon.runtime.records import StoredModelRecord, load_model_record
from tentgent_daemon.runtime.router import resolve_backend

from .config import ServerConfig


@dataclass(frozen=True)
class ChatRequestPayload:
    messages: tuple[Message, ...]
    max_tokens: int | None
    temperature: float | None
    adapter_ref: str | None
    stream: bool


@dataclass(frozen=True)
class SessionSnapshot:
    loaded: bool
    startup_mode: str
    idle_policy: str
    last_activity_at: str | None
    last_release_at: str | None
    last_release_reason: str | None


class RuntimeSession:
    def __init__(self, config: ServerConfig) -> None:
        self._config = config
        self._lock = Lock()
        self._record: StoredModelRecord = load_model_record(
            config.model_ref,
            home=config.home,
        )
        self._backend: ChatBackend = create_backend(resolve_backend(self._record))
        self._loaded = False
        self._last_activity_monotonic: float | None = None
        self._last_activity_at: str | None = None
        self._last_release_at: str | None = None
        self._last_release_reason: str | None = None

    def ensure_loaded(self) -> None:
        with self._lock:
            self._ensure_loaded_locked()

    def release(self, reason: str) -> None:
        with self._lock:
            self._release_locked(reason)

    def snapshot(self) -> SessionSnapshot:
        with self._lock:
            self._release_if_idle_locked()
            return SessionSnapshot(
                loaded=self._loaded,
                startup_mode="lazy" if self._config.lazy_load else "eager",
                idle_policy=(
                    f"release_after:{self._config.idle_seconds}"
                    if self._config.idle_seconds is not None
                    else "keep_warm"
                ),
                last_activity_at=self._last_activity_at,
                last_release_at=self._last_release_at,
                last_release_reason=self._last_release_reason,
            )

    def generate(self, payload: ChatRequestPayload) -> str:
        with self._lock:
            self._release_if_idle_locked()
            self._ensure_loaded_locked()
            request = ChatRequest(
                model_ref=self._config.model_ref,
                messages=payload.messages,
                max_tokens=payload.max_tokens,
                temperature=payload.temperature,
                adapter_ref=payload.adapter_ref,
            )
            text = self._backend.generate(request).text
            self._mark_activity_locked()
            return text

    def _ensure_loaded_locked(self) -> None:
        if self._loaded:
            self._mark_activity_locked()
            return
        self._backend.load(self._record)
        self._loaded = True
        self._mark_activity_locked()

    def _release_if_idle_locked(self) -> None:
        if (
            not self._loaded
            or self._config.idle_seconds is None
            or self._last_activity_monotonic is None
        ):
            return

        if monotonic() - self._last_activity_monotonic >= self._config.idle_seconds:
            self._release_locked("idle_timeout")

    def _release_locked(self, reason: str) -> None:
        if not self._loaded:
            return
        self._backend.release()
        self._loaded = False
        self._last_release_at = _utc_now()
        self._last_release_reason = reason

    def _mark_activity_locked(self) -> None:
        self._last_activity_monotonic = monotonic()
        self._last_activity_at = _utc_now()


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
