from __future__ import annotations

from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from threading import RLock
from time import monotonic
from typing import Any, Generic, TypeVar

from .base import BackendModel
from .records import ModelRecord


ModelT = TypeVar("ModelT", bound=BackendModel)


@dataclass(frozen=True, slots=True)
class ModelResourceKey:
    kind: str
    model_ref: str
    source_path: str


@dataclass(slots=True)
class _LoadedModelResource(Generic[ModelT]):
    model: ModelT
    lock: RLock
    record: ModelRecord
    idle_timeout_seconds: float
    last_used_at: float
    active_leases: int = 0


class ResourceManager(Generic[ModelT]):
    def __init__(
        self,
        *,
        model_factory: Callable[[Any], ModelT],
        model_idle_timeout_seconds: float = 0.0,
    ) -> None:
        self._lock = RLock()
        self._model_resources: dict[
            ModelResourceKey,
            _LoadedModelResource[ModelT],
        ] = {}
        self._default_model_idle_timeout_seconds = model_idle_timeout_seconds
        self._model_factory = model_factory

    @contextmanager
    def lease_model(
        self,
        kind: Any,
        record: ModelRecord,
        *,
        idle_timeout_seconds: float | None = None,
    ) -> Iterator[ModelT]:
        key = ModelResourceKey(
            kind=_kind_value(kind),
            model_ref=record.model_ref,
            source_path=str(record.source_path),
        )
        resource = self._reserve_model_resource(
            key,
            kind,
            record,
            idle_timeout_seconds=idle_timeout_seconds,
        )

        resource.lock.acquire()
        try:
            if not resource.model.is_loaded:
                resource.model.load(record)
            yield resource.model
        finally:
            resource.last_used_at = monotonic()
            resource.lock.release()
            with self._lock:
                resource.active_leases -= 1
            if resource.idle_timeout_seconds == 0:
                self.release_idle()

    def release_idle(self) -> int:
        now = monotonic()
        resources_to_release: list[_LoadedModelResource[ModelT]] = []

        with self._lock:
            for key, resource in list(self._model_resources.items()):
                timeout = resource.idle_timeout_seconds
                if timeout < 0:
                    continue
                if resource.active_leases > 0:
                    continue
                if now - resource.last_used_at < timeout:
                    continue
                if not resource.lock.acquire(blocking=False):
                    continue
                self._model_resources.pop(key, None)
                resources_to_release.append(resource)

        for resource in resources_to_release:
            try:
                resource.model.release()
            finally:
                resource.lock.release()

        return len(resources_to_release)

    def release_all(self) -> None:
        with self._lock:
            resources = list(self._model_resources.values())
            self._model_resources.clear()

        for resource in resources:
            with resource.lock:
                resource.model.release()

    def snapshot(self) -> dict[str, Any]:
        with self._lock:
            model_resources = [
                {
                    "kind": key.kind,
                    "model_ref": key.model_ref,
                    "source_path": key.source_path,
                    "loaded": resource.model.is_loaded,
                    "active_leases": resource.active_leases,
                    "idle_timeout_seconds": resource.idle_timeout_seconds,
                    "idle_age_seconds": round(monotonic() - resource.last_used_at, 3),
                }
                for key, resource in self._model_resources.items()
            ]

        return {
            "default_model_idle_timeout_seconds": (
                self._default_model_idle_timeout_seconds
            ),
            "model_resource_count": len(model_resources),
            "model_resources": model_resources,
        }

    def _reserve_model_resource(
        self,
        key: ModelResourceKey,
        kind: Any,
        record: ModelRecord,
        *,
        idle_timeout_seconds: float | None,
    ) -> _LoadedModelResource[ModelT]:
        with self._lock:
            resource = self._model_resources.get(key)
            if resource is not None:
                if idle_timeout_seconds is not None:
                    resource.idle_timeout_seconds = idle_timeout_seconds
                resource.active_leases += 1
                return resource

            resource = _LoadedModelResource(
                model=self._model_factory(kind),
                lock=RLock(),
                record=record,
                idle_timeout_seconds=(
                    self._default_model_idle_timeout_seconds
                    if idle_timeout_seconds is None
                    else idle_timeout_seconds
                ),
                last_used_at=monotonic(),
            )
            self._model_resources[key] = resource
            resource.active_leases += 1
            return resource


def _kind_value(kind: Any) -> str:
    value = getattr(kind, "value", None)
    if isinstance(value, str):
        return value
    return str(kind)
