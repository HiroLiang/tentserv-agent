from __future__ import annotations

import os
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from threading import Lock
from time import monotonic

from tentgent_daemon.backends import ChatBackend, create_backend
from tentgent_daemon.providers import (
    ProviderChatClient,
    ProviderChatRequest,
    create_provider_chat_client,
)
from tentgent_daemon.runtime.adapters import (
    AdapterExecutionNotImplementedError,
    StoredAdapterRecord,
    load_adapter_record,
    validate_adapter_for_model,
)
from tentgent_daemon.runtime.chat import ChatRequest, Message
from tentgent_daemon.runtime.records import StoredModelRecord, load_model_record
from tentgent_daemon.runtime.router import BackendKind, resolve_backend

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
        self._record: StoredModelRecord | None = None
        self._backend_kind: BackendKind | None = None
        self._backend: ChatBackend | None = None
        self._provider_client: ProviderChatClient | None = None
        if config.is_cloud:
            self._provider_client = create_provider_chat_client(
                _require_cloud_field(config.provider, "provider"),
                _read_provider_api_key(config),
            )
        else:
            model_ref = _require_local_model_ref(config)
            self._record = load_model_record(
                model_ref,
                home=config.home,
            )
            self._backend_kind = resolve_backend(self._record)
            self._backend = create_backend(self._backend_kind)
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
            if self._config.is_cloud:
                startup_mode = "cloud_proxy"
                idle_policy = "stateless_proxy"
            else:
                startup_mode = "lazy" if self._config.lazy_load else "eager"
                idle_policy = (
                    f"release_after:{self._config.idle_seconds}"
                    if self._config.idle_seconds is not None
                    else "keep_warm"
                )
            return SessionSnapshot(
                loaded=self._loaded,
                startup_mode=startup_mode,
                idle_policy=idle_policy,
                last_activity_at=self._last_activity_at,
                last_release_at=self._last_release_at,
                last_release_reason=self._last_release_reason,
            )

    def generate(self, payload: ChatRequestPayload) -> str:
        with self._lock:
            if self._config.is_cloud:
                return self._generate_cloud_locked(payload)

            self._release_if_idle_locked()
            adapter = self._resolve_adapter_request_locked(payload.adapter_ref)
            self._ensure_loaded_locked()
            assert self._backend is not None
            self._backend.select_adapter(adapter)
            request = ChatRequest(
                model_ref=_require_local_model_ref(self._config),
                messages=payload.messages,
                max_tokens=payload.max_tokens,
                temperature=payload.temperature,
                adapter_ref=adapter.adapter_ref if adapter else None,
            )
            text = self._backend.generate(request).text
            self._mark_activity_locked()
            return text

    def _resolve_adapter_request_locked(
        self,
        adapter_ref: str | None,
    ) -> StoredAdapterRecord | None:
        if not adapter_ref:
            return None

        assert self._record is not None
        assert self._backend_kind is not None
        adapter = load_adapter_record(adapter_ref, home=self._config.home)
        validate_adapter_for_model(adapter, self._record, self._backend_kind)
        return adapter

    def _ensure_loaded_locked(self) -> None:
        if self._config.is_cloud:
            return
        if self._loaded:
            self._mark_activity_locked()
            return
        assert self._backend is not None
        assert self._record is not None
        self._backend.load(self._record)
        self._loaded = True
        self._mark_activity_locked()

    def _release_if_idle_locked(self) -> None:
        if self._config.is_cloud:
            return
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
        assert self._backend is not None
        self._backend.release()
        self._loaded = False
        self._last_release_at = _utc_now()
        self._last_release_reason = reason

    def _mark_activity_locked(self) -> None:
        self._last_activity_monotonic = monotonic()
        self._last_activity_at = _utc_now()

    def _generate_cloud_locked(self, payload: ChatRequestPayload) -> str:
        if payload.adapter_ref:
            raise AdapterExecutionNotImplementedError(
                "cloud provider runtimes do not support adapter_ref"
            )
        assert self._provider_client is not None
        response = self._provider_client.generate(
            ProviderChatRequest(
                model=_require_cloud_field(self._config.provider_model, "provider_model"),
                messages=payload.messages,
                max_tokens=payload.max_tokens,
                temperature=payload.temperature,
            )
        )
        self._mark_activity_locked()
        return response.text


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _require_local_model_ref(config: ServerConfig) -> str:
    if not config.model_ref:
        raise ValueError("local server config is missing model_ref")
    return config.model_ref


def _require_cloud_field(value: str | None, field: str) -> str:
    if not value:
        raise ValueError(f"cloud server config is missing {field}")
    return value


def _read_provider_api_key(config: ServerConfig) -> str:
    provider = _require_cloud_field(config.provider, "provider")
    env_var = _provider_env_var(provider)
    value = os.environ.get(env_var, "").strip()
    if not value:
        raise ValueError(f"cloud provider API key is missing from {env_var}")
    return value


def _provider_env_var(provider: str) -> str:
    match provider:
        case "openai":
            return "OPENAI_API_KEY"
        case "anthropic" | "claude":
            return "ANTHROPIC_API_KEY"
        case _:
            raise ValueError(f"unsupported cloud provider `{provider}`")
